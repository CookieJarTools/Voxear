use std::net::SocketAddr;

use voxear_net::MemoryNetwork;

use super::*;

const TICK: f32 = 1.0 / 60.0;

fn settings(conditions: Conditions) -> NetSettings {
    NetSettings { conditions, ..NetSettings::for_project("Tests", "1.0", &NetworkSettings::default()) }
}

/// A server, and clients joined to it through `network`, each in its own
/// world.
fn session(network: &MemoryNetwork, clients: usize, conditions: Conditions) -> (World, Vec<World>) {
    session_with(network, clients, conditions, NetAuthority::Server)
}

fn session_with(network: &MemoryNetwork, clients: usize, conditions: Conditions, authority: NetAuthority) -> (World, Vec<World>) {
    let mut server = World::new();
    let address: SocketAddr = "10.0.0.1:7777".parse().unwrap();
    let served = NetSettings { authority, ..settings(conditions) };
    serve_on(&mut server, served, Box::new(network.bind(address)), false).unwrap();
    let clients = (0..clients)
        .map(|n| {
            let mut world = World::new();
            let transport = network.bind(format!("10.0.1.{}:5000", n + 1).parse().unwrap());
            connect_on(&mut world, settings(conditions), address, Box::new(transport)).unwrap();
            world
        })
        .collect();
    (server, clients)
}

/// What a client's fixed steps have saved up, as the engine keeps it.
struct Pace(f32);

/// One frame everywhere, a tick long, each client holding `pressing` on its
/// one action. A client runs as many ticks as its pace makes due, as the
/// engine's fixed steps do.
fn frame(server: &mut World, clients: &mut [World], pressing: &[(bool, bool)]) {
    frame_begin(server, TICK, &[]);
    fixed_tick(server);
    frame_end(server);
    for (world, &(held, pressed)) in clients.iter_mut().zip(pressing) {
        frame_begin(world, TICK, &[ActionState { held, pressed, ..ActionState::default() }]);
        let saved = world.resource::<Pace>().map_or(0.0, |pace| pace.0) + TICK * time_scale(world);
        let mut saved = saved;
        while saved >= TICK - 1e-6 {
            fixed_tick(world);
            saved -= TICK;
        }
        world.insert_resource(Pace(saved));
        frame_end(world);
    }
}

fn session_of(world: &World) -> &NetSession {
    world.resource::<NetSession>().unwrap()
}

#[test]
fn clients_join_hear_of_each_other_and_their_input_reaches_the_server() {
    let network = MemoryNetwork::new();
    let (mut server, mut clients) = session(&network, 2, Conditions::PERFECT);
    for _ in 0..30 {
        frame(&mut server, &mut clients, &[(false, false); 2]);
    }
    let joined: Vec<NetEvent> = take_events(&mut server);
    assert_eq!(joined, [NetEvent::PlayerJoined(PlayerId(1)), NetEvent::PlayerJoined(PlayerId(2))]);
    for (n, client) in clients.iter_mut().enumerate() {
        let session = session_of(client);
        assert!(session.is_connected());
        assert_eq!(session.local_player(), Some(PlayerId(n as u16 + 1)));
        assert_eq!(session.players().len(), 2, "each hears of the other");
        assert_eq!(take_events(client)[0], NetEvent::Connected);
    }

    // The first player presses for one tick, then holds; the server sees one press and then holding.
    frame(&mut server, &mut clients, &[(true, true), (false, false)]);
    let mut presses = 0;
    let mut held = 0;
    for _ in 0..20 {
        frame(&mut server, &mut clients, &[(true, false), (false, false)]);
        let seen = session_of(&server).input(PlayerId(1), 0);
        presses += usize::from(seen.pressed);
        held += usize::from(seen.held);
        assert!(!session_of(&server).input(PlayerId(2), 0).held, "the other player pressed nothing");
    }
    assert_eq!(presses, 1, "pressed once");
    assert!(held >= 15, "then held: {held} ticks");

    // The second leaves, and the first hears of it.
    stop(&mut clients[1]);
    for _ in 0..10 {
        frame(&mut server, &mut clients[..1], &[(false, false)]);
    }
    assert!(take_events(&mut server).contains(&NetEvent::PlayerLeft(PlayerId(2))));
    assert!(take_events(&mut clients[0]).contains(&NetEvent::PlayerLeft(PlayerId(2))));
    assert_eq!(session_of(&server).players(), [PlayerId(1)]);
}

#[test]
fn a_client_keeps_a_tick_or_two_ahead_through_a_bad_network() {
    let network = MemoryNetwork::new();
    let (mut server, mut clients) = session(&network, 1, Conditions::BAD);
    for _ in 0..600 {
        frame(&mut server, &mut clients, &[(false, false)]);
    }
    assert!(session_of(&clients[0]).is_connected());
    // Over the next few seconds, how far ahead its commands arrive.
    let mut aheads = Vec::new();
    let mut taps = 0;
    for n in 0..300 {
        let tap = n % 30 == 0;
        frame(&mut server, &mut clients, &[(tap, tap)]);
        let tick = session_of(&server).tick;
        aheads.push(session_of(&server).buffers[&PlayerId(1)].ahead(tick));
        taps += usize::from(session_of(&server).input(PlayerId(1), 0).pressed);
    }
    aheads.sort();
    let median = aheads[aheads.len() / 2];
    assert!((1..=4).contains(&median), "commands arrive about two ticks early: {median}");
    assert!(taps >= 9, "every tap reached the server but those at the very end: {taps} of 10");
}

/// A server Obj with a NetObject, shared with players.
fn shared(server: &mut World, position: voxear_core::math::Vec3, object: NetObject) -> voxear_ecs::Entity {
    let entity = server.spawn((voxear_scene::Transform::from_position(position), object));
    track(server, entity, Origin::Prefab("test".into())).unwrap();
    entity
}

/// Makes what the server asked each client to make, as the game does.
fn make_spawns(clients: &mut [World]) {
    for client in clients {
        for request in take_spawns(client) {
            let entity = client.spawn((voxear_scene::Transform::from_position(request.state.position),));
            bind(client, &request, entity);
        }
        for entity in take_despawns(client) {
            client.despawn(entity);
        }
    }
}

fn position_of(world: &World, id: NetId) -> Option<voxear_core::math::Vec3> {
    world
        .query_ref::<(&Networked, &voxear_scene::Transform)>()
        .into_iter()
        .find(|(n, _)| n.id == id)
        .map(|(_, t)| t.position)
}

#[test]
fn a_shared_obj_is_made_on_each_client_and_follows_the_servers_smoothly() {
    let network = MemoryNetwork::new();
    let (mut server, mut clients) = session(&network, 2, Conditions::AVERAGE);
    let mover = shared(&mut server, voxear_core::math::Vec3::ZERO, NetObject::default());
    let id = server.get::<Networked>(mover).unwrap().id;
    let mut seen = Vec::new();
    for n in 0..300 {
        // A metre a second along X.
        server.get_mut::<voxear_scene::Transform>(mover).unwrap().position.x = n as f32 / 60.0;
        frame(&mut server, &mut clients, &[(false, false); 2]);
        make_spawns(&mut clients);
        if n > 120 {
            seen.push(position_of(&clients[0], id).expect("made on the client").x);
        }
    }
    let at = position_of(&clients[1], id).unwrap().x;
    let now = 299.0 / 60.0;
    assert!(now - at > 0.02 && now - at < 0.25, "a little behind the server, as shown: {at} against {now}");
    // Smooth: it moves forward every frame by about a frame's worth, never jumping or stalling long.
    let steps: Vec<f32> = seen.windows(2).map(|w| w[1] - w[0]).collect();
    assert!(steps.iter().all(|s| *s > -0.001 && *s < 0.06), "smooth: {steps:?}");
    assert!(steps.iter().filter(|s| **s < 0.002).count() < 20, "rarely still");

    // Taken away on the server, gone on the clients.
    server.despawn(mover);
    for _ in 0..30 {
        frame(&mut server, &mut clients, &[(false, false); 2]);
        make_spawns(&mut clients);
    }
    assert!(position_of(&clients[0], id).is_none());
}

#[test]
fn a_player_is_sent_only_what_is_in_range_of_their_own() {
    let network = MemoryNetwork::new();
    let (mut server, mut clients) = session(&network, 1, Conditions::PERFECT);
    for _ in 0..30 {
        frame(&mut server, &mut clients, &[(false, false)]);
    }
    let me = shared(&mut server, voxear_core::math::Vec3::ZERO, NetObject::default());
    set_owner(&mut server, me, Some(PlayerId(1)));
    let near = shared(&mut server, voxear_core::math::Vec3::new(5.0, 0.0, 0.0), NetObject { range: 20.0, ..NetObject::default() });
    let far = shared(&mut server, voxear_core::math::Vec3::new(50.0, 0.0, 0.0), NetObject { range: 20.0, ..NetObject::default() });
    let id = |e| server.get::<Networked>(e).unwrap().id;
    let (near, far) = (id(near), id(far));
    for _ in 0..30 {
        frame(&mut server, &mut clients, &[(false, false)]);
        make_spawns(&mut clients);
    }
    assert!(position_of(&clients[0], near).is_some(), "in range");
    assert!(position_of(&clients[0], far).is_none(), "out of range");
    // Walking over brings it, walking back takes it away.
    server.get_mut::<voxear_scene::Transform>(me).unwrap().position.x = 45.0;
    for _ in 0..30 {
        frame(&mut server, &mut clients, &[(false, false)]);
        make_spawns(&mut clients);
    }
    assert!(position_of(&clients[0], far).is_some() && position_of(&clients[0], near).is_none());
}

#[test]
fn a_crowd_too_big_for_a_snapshot_is_shared_by_turns_within_the_budget() {
    let network = MemoryNetwork::new();
    let (mut server, mut clients) = session(&network, 1, Conditions::PERFECT);
    for _ in 0..30 {
        frame(&mut server, &mut clients, &[(false, false)]);
    }
    let crowd: Vec<_> =
        (0..300).map(|n| shared(&mut server, voxear_core::math::Vec3::new(n as f32, 0.0, 0.0), NetObject::default())).collect();
    let mut last_moved = std::collections::HashMap::new();
    for frame_number in 0..240u32 {
        for (n, &entity) in crowd.iter().enumerate() {
            server.get_mut::<voxear_scene::Transform>(entity).unwrap().position.z = frame_number as f32 * 0.1 + n as f32;
        }
        frame(&mut server, &mut clients, &[(false, false)]);
        make_spawns(&mut clients);
        if frame_number >= 120 {
            for &entity in &crowd {
                let id = server.get::<Networked>(entity).unwrap().id;
                let z = position_of(&clients[0], id).map(|p| p.z);
                if let Some(z) = z {
                    let entry = last_moved.entry(id).or_insert((z, frame_number, 0u32));
                    if (z - entry.0).abs() > 1e-4 {
                        entry.2 = entry.2.max(frame_number - entry.1);
                        (entry.0, entry.1) = (z, frame_number);
                    }
                }
            }
        }
    }
    assert_eq!(last_moved.len(), 300, "every one made");
    let longest = last_moved.values().map(|(_, _, gap)| *gap).max().unwrap();
    assert!(longest < 30, "none waits more than half a second for news: {longest} frames");
    let sent = session_of(&server).stats().sent_bytes_per_second;
    assert!(sent < 64.0 * 1024.0 * 1.3, "within the budget: {sent} bytes a second");
}

#[test]
fn rpcs_reach_the_server_from_a_player_and_players_from_the_server() {
    let network = MemoryNetwork::new();
    let (mut server, mut clients) = session(&network, 2, Conditions::AVERAGE);
    for _ in 0..60 {
        frame(&mut server, &mut clients, &[(false, false); 2]);
    }
    let ball = shared(&mut server, voxear_core::math::Vec3::ZERO, NetObject::default());
    // One made in the same tick as the Obj still reaches those it is for.
    assert!(rpc(&mut server, ball, 7, vec![1], RpcTarget::Players, true));
    for _ in 0..30 {
        frame(&mut server, &mut clients, &[(false, false); 2]);
        make_spawns(&mut clients);
    }
    for client in &mut clients {
        let came = take_rpc(client).expect("the server's RPC");
        assert_eq!((came.key, came.caller, came.bytes), (7, None, vec![1]));
        assert_eq!(Some(came.entity), entity_of(client, NetId(1)));
    }

    // To the server, with the tick it was made on; then to one player only.
    let on_client = entity_of(&clients[1], NetId(1)).unwrap();
    let made_on = session_of(&clients[1]).tick;
    assert!(rpc(&mut clients[1], on_client, 9, vec![2, 3], RpcTarget::Server, true));
    assert!(!rpc(&mut clients[1], on_client, 9, Vec::new(), RpcTarget::Players, true), "only the server calls players");
    let first = session_of(&clients[0]).local_player().unwrap();
    let second = session_of(&clients[1]).local_player();
    assert!(rpc(&mut server, ball, 8, Vec::new(), RpcTarget::Player(first), true));
    for _ in 0..30 {
        frame(&mut server, &mut clients, &[(false, false); 2]);
    }
    let came = take_rpc(&mut server).expect("the player's RPC");
    assert_eq!((came.entity, came.key, came.caller, came.tick), (ball, 9, second, made_on));
    assert_eq!(take_rpc(&mut server), None);
    assert_eq!(take_rpc(&mut clients[0]).map(|r| r.key), Some(8));
    assert_eq!(take_rpc(&mut clients[1]), None, "not for them");
}

#[test]
fn a_players_script_sending_a_mob_the_server_moves_reaches_the_server() {
    use crate::nav_agent::{NavAgent, direct};
    use voxear_core::math::Vec3;
    let network = MemoryNetwork::new();
    let (mut server, mut clients) = session(&network, 1, Conditions::AVERAGE);
    for _ in 0..60 {
        frame(&mut server, &mut clients, &[(false, false)]);
    }
    let mob = shared(&mut server, Vec3::ZERO, NetObject::default());
    server.insert(mob, NavAgent::default());
    for _ in 0..30 {
        frame(&mut server, &mut clients, &[(false, false)]);
        make_spawns(&mut clients);
    }
    let on_client = entity_of(&clients[0], NetId(1)).unwrap();
    assert!(!has_authority(&clients[0], on_client), "the server moves it");
    clients[0].insert(on_client, NavAgent { destination: Some(Vec3::new(20.5, 6.0, -3.25)), ..NavAgent::default() });
    direct(&mut clients[0], on_client);
    for _ in 0..30 {
        frame(&mut server, &mut clients, &[(false, false)]);
    }
    let asked = take_nav_asked(&mut server);
    assert_eq!(asked.len(), 1, "{asked:?}");
    assert_eq!(asked[0].entity, mob);
    let (to, dimension) = asked[0].to.expect("somewhere to go");
    assert!(to.distance(Vec3::new(20.5, 6.0, -3.25)) < 0.01 && dimension.is_level(), "{to}");
    assert_eq!(asked[0].player, session_of(&clients[0]).local_player().unwrap());
    // Stopped, it is told so; the server's own word is no request.
    clients[0].get_mut::<NavAgent>(on_client).unwrap().stopped = true;
    direct(&mut clients[0], on_client);
    direct(&mut server, mob);
    for _ in 0..30 {
        frame(&mut server, &mut clients, &[(false, false)]);
    }
    let asked = take_nav_asked(&mut server);
    assert_eq!(asked.iter().map(|asked| asked.stopped).collect::<Vec<_>>(), [true]);
}

/// Moves a ball a centimetre along x, as a `[Predicted]` behaviour would each tick.
fn roll(world: &mut World, ball: voxear_ecs::Entity) {
    if let Some(transform) = world.get_mut::<voxear_scene::Transform>(ball) {
        transform.position.x += 0.01;
    }
}

/// A client's frame with its own ball predicted: corrections first, as the engine makes them, then its ticks.
fn predicted_frame(client: &mut World) {
    predicted_frame_rolling(client, roll);
}

/// The same, the ball rolled by `roll` each tick.
fn predicted_frame_rolling(client: &mut World, roll: fn(&mut World, voxear_ecs::Entity)) {
    frame_begin(client, TICK, &[]);
    make_spawns(std::slice::from_mut(client));
    mark_predicting(client, &|_| true);
    for correction in corrections(client) {
        begin_replay(client, &correction);
        while replay_tick(client) {
            roll(client, correction.entity);
            record(client);
        }
        end_replay(client, &correction);
    }
    let saved = client.resource::<Pace>().map_or(0.0, |pace| pace.0) + TICK * time_scale(client);
    let mut saved = saved;
    while saved >= TICK - 1e-6 {
        fixed_tick(client);
        if let Some(ball) = entity_of(client, NetId(1)).filter(|ball| client.has::<Predicting>(*ball)) {
            roll(client, ball);
        }
        record(client);
        saved -= TICK;
    }
    client.insert_resource(Pace(saved));
    frame_end(client);
}

#[test]
fn a_players_own_obj_moves_at_once_and_is_put_right_when_the_server_disagrees() {
    let network = MemoryNetwork::new();
    let (mut server, mut clients) = session(&network, 1, Conditions::GOOD);
    for _ in 0..60 {
        frame(&mut server, &mut clients, &[(false, false)]);
    }
    let ball = shared(&mut server, voxear_core::math::Vec3::ZERO, NetObject::default());
    set_owner(&mut server, ball, Some(PlayerId(1)));
    let client = &mut clients[0];
    let mut put_right = Vec::new();
    for n in 0..400 {
        frame_begin(&mut server, TICK, &[]);
        fixed_tick(&mut server);
        roll(&mut server, ball);
        // The server has its own idea once: the ball jumps.
        if n == 200 {
            server.get_mut::<voxear_scene::Transform>(ball).unwrap().position.x += 5.0;
        }
        frame_end(&mut server);
        predicted_frame(client);
        put_right.push(session_of(client).stats().corrections);
    }
    let on_client = entity_of(client, NetId(1)).expect("the ball, made on the client");
    assert!(client.has::<Predicting>(on_client));
    let (server_x, server_tick) = (server.get::<voxear_scene::Transform>(ball).unwrap().position.x, session_of(&server).tick);
    let client_tick = session_of(client).tick;
    let client_x = session_of(client).prediction.history[&NetId(1)].back().unwrap().1.position.x;
    // Ahead of the server by as many ticks as it runs ahead, not behind it by a round trip.
    let expected = server_x + (client_tick - server_tick) as f32 * 0.01;
    assert!((client_x - expected).abs() < 0.02, "{client_x} where the server will have {expected}");
    let before = put_right[190];
    let after = put_right[399] - before;
    assert!(before <= 2, "put right at most as it joined: {before}");
    assert!((1..=3).contains(&after), "put right once the ball jumped: {after}");
    assert!(session_of(client).prediction.ids.contains(&NetId(1)));
}

#[test]
fn a_ray_as_a_player_saw_it_meets_what_they_saw_where_they_saw_it() {
    use voxear_physics::scene::{Collider, PhysicsScene};
    let network = MemoryNetwork::new();
    let (mut server, mut clients) = session(&network, 1, Conditions::AVERAGE);
    for _ in 0..60 {
        frame(&mut server, &mut clients, &[(false, false)]);
    }
    // A box running across, twelve metres a second, which the player sees a moment late.
    let target = shared(&mut server, voxear_core::math::Vec3::new(-30.0, 1.0, 0.0), NetObject::default());
    server.insert(target, Collider::default());
    server.insert_resource(PhysicsScene::default());
    let mut xs = HashMap::new();
    for _ in 0..240 {
        frame_begin(&mut server, TICK, &[]);
        fixed_tick(&mut server);
        server.get_mut::<voxear_scene::Transform>(target).unwrap().position.x += 0.2;
        let tick = session_of(&server).tick;
        xs.insert(tick, server.get::<voxear_scene::Transform>(target).unwrap().position.x);
        after_tick(&mut server);
        frame_end(&mut server);
        frame(&mut World::new(), &mut clients, &[(false, false)]);
        make_spawns(&mut clients);
    }
    let now = session_of(&server).tick;
    let seen = session_of(&server).rewind.view(PlayerId(1), now).expect("what the player saw");
    let seen = seen.max(now as f64 - MOST_REWIND * 60.0);
    assert!(now as f64 - seen >= 4.0, "the player sees the world a few ticks late: {} ticks", now as f64 - seen);
    let then = xs[&(seen.floor() as u64)] + 0.2 * seen.fract() as f32;
    let x = xs[&now];
    let at = |x: f32| voxear_core::math::Vec3::new(x, 1.0, -5.0);
    let plain = |world: &mut World, x: f32| {
        let mut scene = world.remove_resource::<PhysicsScene>().unwrap();
        scene.ready(world);
        let hit = scene.raycast(at(x), voxear_core::math::Vec3::Z, 10.0, u32::MAX, false).map(|hit| hit.obj);
        world.insert_resource(scene);
        hit
    };
    let seen_by = |world: &mut World, x: f32| {
        raycast_as_seen_by(world, PlayerId(1), now, at(x), voxear_core::math::Vec3::Z, 10.0, u32::MAX, false).map(|hit| hit.obj)
    };
    assert_eq!(plain(&mut server, then), None, "where they saw it, it has gone by now");
    assert_eq!(seen_by(&mut server, then), Some(target), "but as they saw it, it was there");
    assert_eq!(seen_by(&mut server, x), None, "and as they saw it, it had not got here yet");
    assert_eq!(plain(&mut server, x), Some(target), "and it is put back where it is");
}

#[test]
fn with_shared_authority_players_run_their_own_objs_claim_others_and_lose_them_on_leaving() {
    let network = MemoryNetwork::new();
    let (mut server, mut clients) = session_with(&network, 2, Conditions::GOOD, NetAuthority::Shared);
    for _ in 0..60 {
        frame(&mut server, &mut clients, &[(false, false); 2]);
    }
    let (first, second) = (session_of(&clients[0]).local_player(), session_of(&clients[1]).local_player());
    let crate_ = shared(&mut server, voxear_core::math::Vec3::ZERO, NetObject::default());
    set_owner(&mut server, crate_, first);
    let spare = shared(&mut server, voxear_core::math::Vec3::new(5.0, 0.0, 0.0), NetObject::default());
    for _ in 0..30 {
        frame(&mut server, &mut clients, &[(false, false); 2]);
        make_spawns(&mut clients);
    }
    // The first player moves their crate on their own machine; the server and the other player follow.
    let mine = entity_of(&clients[0], NetId(1)).unwrap();
    for _ in 0..60 {
        clients[0].get_mut::<voxear_scene::Transform>(mine).unwrap().position.x += 0.05;
        frame(&mut server, &mut clients, &[(false, false); 2]);
    }
    let x = |world: &World, entity| world.get::<voxear_scene::Transform>(entity).unwrap().position.x;
    let theirs = entity_of(&clients[1], NetId(1)).unwrap();
    assert!((x(&clients[0], mine) - 3.0).abs() < 1e-3, "the owner's machine moved it, and nothing moved it back");
    assert!((x(&server, crate_) - 3.0).abs() < 0.3, "the server follows its owner: {}", x(&server, crate_));
    assert!(x(&clients[1], theirs) > 2.0, "and so does the other player: {}", x(&clients[1], theirs));
    assert!(has_authority(&clients[0], mine) && !has_authority(&server, crate_) && !has_authority(&clients[1], theirs));

    // The second claims the spare, and everyone hears it is theirs.
    let wanted = entity_of(&clients[1], NetId(2)).unwrap();
    claim(&mut clients[1], wanted);
    for _ in 0..30 {
        frame(&mut server, &mut clients, &[(false, false); 2]);
    }
    assert_eq!(server.get::<Networked>(spare).unwrap().owner, second);
    assert_eq!(clients[0].get::<Networked>(entity_of(&clients[0], NetId(2)).unwrap()).unwrap().owner, second);

    // The first leaves, and the server takes their crate back: as soon as it hears, or once they time out.
    stop(&mut clients[0]);
    for _ in 0..600 {
        frame(&mut server, &mut clients[1..], &[(false, false)]);
        if !session_of(&server).players().contains(&first.unwrap()) {
            break;
        }
    }
    for _ in 0..10 {
        frame(&mut server, &mut clients[1..], &[(false, false)]);
    }
    assert_eq!(server.get::<Networked>(crate_).unwrap().owner, None);
    assert_eq!(clients[1].get::<Networked>(theirs).unwrap().owner, None);
    assert!(has_authority(&server, crate_));
}

#[test]
fn a_players_aim_reaches_the_server_with_their_input() {
    let network = MemoryNetwork::new();
    let (mut server, mut clients) = session(&network, 1, Conditions::GOOD);
    for _ in 0..60 {
        frame(&mut server, &mut clients, &[(false, false)]);
    }
    set_aim(&mut clients[0], [135.0, -20.0]);
    for _ in 0..20 {
        frame(&mut server, &mut clients, &[(true, false)]);
    }
    let session = session_of(&server);
    assert_eq!(session.aim(PlayerId(1)), [135.0, -20.0]);
    assert!(session.input(PlayerId(1), 0).held, "the actions read as before, the aim after them");
    assert_eq!(session.input(PlayerId(1), 1), ActionState::default(), "the aim is not an action");
}

/// Where a networked Obj is on `world`, measured from the level's middle.
fn level_position_of(world: &World, id: NetId) -> Option<voxear_core::math::DVec3> {
    position_of(world, id).map(|at| voxear_scene::WorldOrigin::of(world).to_level(at))
}

#[test]
fn players_far_out_see_the_servers_objs_where_they_are_as_their_own_middles_move() {
    use voxear_core::math::{IVec3, Vec3};
    let network = MemoryNetwork::new();
    let (mut server, mut clients) = session(&network, 2, Conditions::GOOD);
    // 20 km out, past where a position once fitted, moving a metre a second.
    let start = Vec3::new(20_000.0, 0.0, 3000.0);
    let mover = shared(&mut server, start, NetObject::default());
    let id = server.get::<Networked>(mover).unwrap().id;
    let mut seen = Vec::new();
    for n in 0..300 {
        server.get_mut::<voxear_scene::Transform>(mover).unwrap().position.x = start.x + n as f32 / 60.0;
        // The first player's middle follows them out to it, then on a kilometre; the second's stays put.
        if n == 60 {
            crate::origin::shift(&mut clients[0], IVec3::new(-20_480, 0, -3072));
        }
        if n == 150 {
            crate::origin::shift(&mut clients[0], IVec3::new(-1024, 0, 0));
        }
        frame(&mut server, &mut clients, &[(false, false); 2]);
        make_spawns(&mut clients);
        if n > 30 {
            seen.push(level_position_of(&clients[0], id).expect("made on the first player's machine"));
        }
    }
    // Moving on smoothly through both moves, a frame's worth a frame, never jumping.
    let steps: Vec<f64> = seen.windows(2).map(|w| w[1].x - w[0].x).collect();
    assert!(steps.iter().all(|s| *s > -0.001 && *s < 0.06), "smooth across the moves: {steps:?}");
    assert!(seen.windows(2).all(|w| (w[1].z - 3000.0).abs() < 1e-3), "and nowhere else");
    // Near the first player's middle in their world, where the second sees it from the level's.
    let local = position_of(&clients[0], id).unwrap();
    assert!(local.distance(Vec3::new(-1499.0, 0.0, -72.0)) < 1.0, "near the first player's middle: {local}");
    let (first, second) = (level_position_of(&clients[0], id).unwrap(), level_position_of(&clients[1], id).unwrap());
    // Each shows it a little in the past at its own pace: within a frame's worth of its moving.
    assert!(first.distance(second) < 1.0 / 60.0, "both players see it in one place: {first} and {second}");
}

/// Rolls the ball on by a step a float holds exactly 30 km out as near the
/// middle, so the server far out and its player near their own middle
/// agree to the bit.
fn roll_exactly(world: &mut World, ball: voxear_ecs::Entity) {
    if let Some(transform) = world.get_mut::<voxear_scene::Transform>(ball) {
        transform.position.x += 1.0 / 128.0;
    }
}

#[test]
fn a_players_own_obj_far_out_is_not_put_right_when_their_middle_moves() {
    use voxear_core::math::{IVec3, Vec3};
    let network = MemoryNetwork::new();
    let (mut server, mut clients) = session(&network, 1, Conditions::GOOD);
    for _ in 0..60 {
        frame(&mut server, &mut clients, &[(false, false)]);
    }
    let ball = shared(&mut server, Vec3::new(30_000.0, 0.0, 0.0), NetObject::default());
    set_owner(&mut server, ball, Some(PlayerId(1)));
    let client = &mut clients[0];
    let mut put_right = Vec::new();
    for n in 0..300 {
        frame_begin(&mut server, TICK, &[]);
        fixed_tick(&mut server);
        roll_exactly(&mut server, ball);
        frame_end(&mut server);
        if n == 120 || n == 200 {
            crate::origin::shift(client, IVec3::new(if n == 120 { -30_720 } else { 1024 }, 0, 0));
        }
        predicted_frame_rolling(client, roll_exactly);
        put_right.push(session_of(client).stats().corrections);
    }
    assert_eq!(put_right[299], put_right[100], "never put right for its player's middle moving");
    let on_client = entity_of(client, NetId(1)).unwrap();
    let server_x = server.get::<voxear_scene::Transform>(ball).unwrap().position.x;
    let server_tick = session_of(&server).tick;
    let shown = client.get::<voxear_scene::Transform>(on_client).unwrap().position;
    let client_x = voxear_scene::WorldOrigin::of(client).to_level(shown).x;
    let expected = f64::from(server_x) + (session_of(client).tick - server_tick) as f64 / 128.0;
    assert!((client_x - expected).abs() < 0.02, "{client_x} from the level's middle, the server's soon {expected}");
}

#[test]
fn with_shared_authority_a_player_far_out_moves_their_own_obj_where_the_server_sees_it() {
    use voxear_core::math::{IVec3, Vec3};
    let network = MemoryNetwork::new();
    let (mut server, mut clients) = session_with(&network, 1, Conditions::GOOD, NetAuthority::Shared);
    for _ in 0..60 {
        frame(&mut server, &mut clients, &[(false, false)]);
    }
    let crate_ = shared(&mut server, Vec3::new(50_000.0, 0.0, 0.0), NetObject::default());
    set_owner(&mut server, crate_, session_of(&clients[0]).local_player());
    for _ in 0..30 {
        frame(&mut server, &mut clients, &[(false, false)]);
        make_spawns(&mut clients);
    }
    crate::origin::shift(&mut clients[0], IVec3::new(-50_176, 0, 0));
    let mine = entity_of(&clients[0], NetId(1)).unwrap();
    assert_eq!(clients[0].get::<voxear_scene::Transform>(mine).unwrap().position.x, -176.0);
    for _ in 0..60 {
        clients[0].get_mut::<voxear_scene::Transform>(mine).unwrap().position.x += 0.05;
        frame(&mut server, &mut clients, &[(false, false)]);
    }
    let at = server.get::<voxear_scene::Transform>(crate_).unwrap().position.x;
    assert!((at - 50_003.0).abs() < 0.3, "the server follows its owner from the level's middle: {at}");
}

#[test]
fn the_servers_storms_reach_every_player_as_they_join_as_they_change_and_with_its_clock() {
    use crate::storms::{Storm, Storms};
    let network = MemoryNetwork::new();
    let (mut server, mut clients) = session(&network, 1, Conditions::PERFECT);
    // Storms the server's scripts made before anyone joined.
    let mut storms = Storms::default();
    storms.clock = 500.0;
    let first = storms.start(Storm { middle: [100.0, -40.0], radius: 300.0, velocity: [2.0, 0.0], ..Storm::default() });
    let pool = storms.add_whirlpool([10.0, 20.0], 30.0, 5.0);
    server.insert_resource(storms);
    for _ in 0..30 {
        frame(&mut server, &mut clients, &[(false, false)]);
    }
    let theirs = clients[0].resource::<Storms>().expect("the server's storms");
    let ours = server.resource::<Storms>().unwrap();
    assert_eq!(theirs.made.iter().map(|storm| storm.id).collect::<Vec<_>>(), vec![first]);
    assert_eq!((theirs.made[0].middle, theirs.whirlpools[0].id), ([100.0, -40.0], pool));
    assert!((theirs.clock - ours.clock).abs() < 1.0, "the clock with it: {} and {}", theirs.clock, ours.clock);

    // A storm started on the server reaches the player within a frame or two, and so does one stopped.
    let second = server.resource_mut::<Storms>().unwrap().start(Storm { middle: [-900.0, 0.0], ..Storm::default() });
    for _ in 0..3 {
        frame(&mut server, &mut clients, &[(false, false)]);
    }
    assert!(clients[0].resource::<Storms>().unwrap().get(second).is_some());
    // One a player made for themself lasts only until the server's next arrive.
    clients[0].resource_mut::<Storms>().unwrap().start(Storm::default());
    server.resource_mut::<Storms>().unwrap().stop(first, 0.0);
    for _ in 0..3 {
        frame(&mut server, &mut clients, &[(false, false)]);
    }
    let theirs = clients[0].resource::<Storms>().unwrap();
    assert_eq!(theirs.made.len(), 2);
    assert!(theirs.get(first).unwrap().lasts.is_finite(), "stopped there too");

    // A player joining later is sent them at once, with no change to wait for.
    let transport = network.bind("10.0.1.9:5000".parse().unwrap());
    let mut late = World::new();
    connect_on(&mut late, settings(Conditions::PERFECT), "10.0.0.1:7777".parse().unwrap(), Box::new(transport)).unwrap();
    clients.push(late);
    for _ in 0..30 {
        frame(&mut server, &mut clients, &[(false, false); 2]);
    }
    let late = clients[1].resource::<Storms>().expect("sent as they joined");
    assert_eq!(late.saved().made, server.resource::<Storms>().unwrap().saved().made);
}

/// A dimension of the server's, described as a `.dimension` asset's would be.
fn caves_on(server: &mut World, id: u16) -> voxear_scene::DimensionId {
    let dimension = voxear_scene::DimensionId(id);
    let what = DimensionWhat::Asset { guid: format!("{:032x}", 0xcafe + u64::from(id)), key: 0 };
    describe(server, dimension, what);
    dimension
}

/// As the game does on a player's machine: makes each dimension the server
/// names, numbered here 40 past the server's number for it, and each Obj in
/// the dimension it is in.
fn make_in_dimensions(clients: &mut [World]) {
    for client in clients.iter_mut() {
        for (number, _) in dimensions_to_make(client) {
            dimension_made(client, number, voxear_scene::DimensionId(40 + number));
        }
    }
    for client in clients.iter_mut() {
        for request in take_spawns(client) {
            let entity = client.spawn((voxear_scene::Transform::from_position(request.state.position),));
            voxear_scene::set_dimension(client, entity, request.dimension);
            bind(client, &request, entity);
        }
        for entity in take_despawns(client) {
            client.despawn(entity);
        }
    }
}

/// The server's Obj's dimension on a player's machine, where it has it.
fn dimension_there(client: &World, id: NetId) -> Option<voxear_scene::DimensionId> {
    entity_of(client, id)
        .filter(|entity| client.is_alive(*entity))
        .map(|entity| voxear_scene::dimension_of(client, entity))
}

/// Frames until the server's described dimensions are current: as the game
/// describes them every frame.
fn frames_in_dimensions(server: &mut World, clients: &mut [World], frames: usize, described: &[u16]) {
    for _ in 0..frames {
        forget_described(server);
        for &id in described {
            caves_on(server, id);
        }
        // The edits each player is owed: none in these worlds but a word they are all sent.
        for (player, dimension, _) in take_edits_wanted(server) {
            owe_edits(server, player, dimension, Vec::new());
        }
        frame(server, clients, &vec![(false, false); clients.len()]);
        make_in_dimensions(clients);
        for client in clients.iter_mut() {
            take_edits(client);
        }
    }
}

#[test]
fn players_in_different_dimensions_get_only_the_objs_of_their_own_and_see_them_cross() {
    use voxear_core::math::Vec3;
    let network = MemoryNetwork::new();
    let (mut server, mut clients) = session(&network, 2, Conditions::GOOD);
    for _ in 0..30 {
        frame(&mut server, &mut clients, &[(false, false); 2]);
    }
    let caves = caves_on(&mut server, 3);
    // The first player in the level, the second in the caves, at the same numbers.
    let near = NetObject { range: 20.0, ..NetObject::default() };
    let first = shared(&mut server, Vec3::ZERO, near);
    set_owner(&mut server, first, Some(PlayerId(1)));
    let second = shared(&mut server, Vec3::ZERO, near);
    set_owner(&mut server, second, Some(PlayerId(2)));
    voxear_scene::set_dimension(&mut server, second, caves);
    // Near each, an Obj of each dimension; and one of each with no range.
    let rock = shared(&mut server, Vec3::new(2.0, 0.0, 0.0), near);
    let bat = shared(&mut server, Vec3::new(2.0, 0.0, 0.0), near);
    voxear_scene::set_dimension(&mut server, bat, caves);
    let flag = shared(&mut server, Vec3::new(500.0, 0.0, 0.0), NetObject::default());
    let echo = shared(&mut server, Vec3::new(500.0, 0.0, 0.0), NetObject::default());
    voxear_scene::set_dimension(&mut server, echo, caves);
    let id = |server: &World, entity| server.get::<Networked>(entity).unwrap().id;
    let [first, second, rock, bat, flag, echo] = [first, second, rock, bat, flag, echo].map(|e| id(&server, e));
    frames_in_dimensions(&mut server, &mut clients, 60, &[3]);
    let has = |client: &World, id| dimension_there(client, id);
    let level = voxear_scene::DimensionId::LEVEL;
    let theirs = voxear_scene::DimensionId(41);
    assert_eq!([first, rock, flag].map(|id| has(&clients[0], id)), [Some(level); 3], "the level's, near or everyone's");
    assert_eq!([second, bat, echo].map(|id| has(&clients[0], id)), [None; 3], "nothing of the caves");
    assert_eq!([second, bat, echo].map(|id| has(&clients[1], id)), [Some(theirs); 3], "the caves', in their own");
    assert_eq!(has(&clients[1], rock), None, "the level's rock is near only in numbers");
    assert_eq!(has(&clients[1], flag), Some(level), "every player has the level, and what in it goes to all");
    assert_eq!(has(&clients[1], first), None, "the other player is near only in numbers");

    // The bat flies into the level, past the first player: the second's machine lets it go, the first's makes it.
    let bat_entity = entity_of(&server, bat).unwrap();
    voxear_scene::set_dimension(&mut server, bat_entity, level);
    frames_in_dimensions(&mut server, &mut clients, 20, &[3]);
    assert_eq!((has(&clients[0], bat), has(&clients[1], bat)), (Some(level), None));

    // The first player goes into the caves after the bat flew out: their machine is told of the caves, makes
    // them, and has its own Obj go in, with all the caves have, and the level's rock goes.
    let first_entity = entity_of(&server, first).unwrap();
    voxear_scene::set_dimension(&mut server, first_entity, caves);
    let bytes_before = session_of(&server).stats().sent_bytes_per_second;
    frames_in_dimensions(&mut server, &mut clients, 20, &[3]);
    assert_eq!([first, second, echo].map(|id| has(&clients[0], id)), [Some(theirs); 3]);
    assert_eq!((has(&clients[0], rock), has(&clients[0], bat)), (None, None));
    assert_eq!(has(&clients[1], first), Some(theirs), "and the second player sees them come");
    assert!(bytes_before >= 0.0);
}

#[test]
fn a_dimensions_edits_reach_a_player_nearest_first_before_they_may_go_in() {
    use voxear_core::math::Vec3;
    use voxear_terrain::blocks::edits::BlockEdits;
    let network = MemoryNetwork::new();
    let (mut server, mut clients) = session(&network, 1, Conditions::GOOD);
    for _ in 0..30 {
        frame(&mut server, &mut clients, &[(false, false)]);
    }
    let me = shared(&mut server, Vec3::ZERO, NetObject::default());
    set_owner(&mut server, me, Some(PlayerId(1)));
    // Pieces of a dungeon's edits: 400 chunks with a block each dug, in pieces of 50.
    let pieces: Vec<EditPiece> = (0..8)
        .map(|piece| {
            let mut map = voxear_terrain::blocks::BlockMap::default();
            let mut now = voxear_terrain::blocks::BlockMap::default();
            for n in 0..50 {
                let at = [(piece * 50 + n) * 16, 0, 0];
                map.set(at, 3);
                now.set(at, 3);
                now.set([at[0] + 1, 0, 0], 3);
            }
            let edits = BlockEdits::between(&map, &now);
            EditPiece {
                world: 0,
                numbered: vec![(9, "a block numbered as the game ran".into(), 0)],
                bytes: edits.to_bytes(),
            }
        })
        .collect();
    let caves = caves_on(&mut server, 2);
    // The level's edits first, as they joined.
    for _ in 0..5 {
        frame(&mut server, &mut clients, &[(false, false)]);
        for (player, dimension, _) in take_edits_wanted(&mut server) {
            assert!(dimension.is_level());
            owe_edits(&mut server, player, dimension, Vec::new());
        }
    }
    // Into the caves: the server owes their edits, and the player may not go in till they are in.
    voxear_scene::set_dimension(&mut server, me, caves);
    let mut arrived = Vec::new();
    let mut asked = 0;
    let mut ready_after = None;
    for n in 0..120 {
        forget_described(&mut server);
        caves_on(&mut server, 2);
        for (player, dimension, _) in take_edits_wanted(&mut server) {
            assert_eq!(dimension, caves);
            asked += 1;
            owe_edits(&mut server, player, dimension, pieces.clone());
        }
        frame(&mut server, &mut clients, &[(false, false)]);
        make_in_dimensions(&mut clients);
        arrived.extend(take_edits(&mut clients[0]));
        if ready_after.is_none() && may_cross(&server, me, caves) {
            ready_after = Some(n);
        }
    }
    assert_eq!(asked, 1, "asked for once");
    assert_eq!(arrived.len(), pieces.len(), "every piece came");
    assert!(arrived.iter().zip(&pieces).all(|(came, sent)| came.piece == *sent), "in the order sent, nearest first");
    assert!(arrived.iter().all(|came| came.dimension == voxear_scene::DimensionId(41)), "for the caves as made here");
    assert!(edits_in(&clients[0], voxear_scene::DimensionId(41)));
    let after = ready_after.expect("ready once they are in");
    let sent = session_of(&server).stats().edit_bytes;
    let made: usize = pieces.iter().map(|piece| piece.bytes.len()).sum();
    println!(
        "400 chunks with a block changed: {made} bytes of edits, {sent} sent with their messages, in {after} frames"
    );
    assert!(sent as usize >= made && (sent as usize) < made + 600, "{sent} bytes for {made}");
    // Half the budget a second at most: 32 kB/s.
    let seconds = after as f64 / 60.0;
    assert!(sent as f64 / seconds <= 34.0 * 1024.0, "{:.1} kB/s", sent as f64 / seconds / 1024.0);
}

#[test]
fn with_shared_authority_a_crossing_is_taken_by_a_portal_and_refused_without_one() {
    use voxear_core::math::{Quat, Vec3};
    let network = MemoryNetwork::new();
    let (mut server, mut clients) = session_with(&network, 1, Conditions::GOOD, NetAuthority::Shared);
    for _ in 0..30 {
        frame(&mut server, &mut clients, &[(false, false)]);
    }
    let caves = caves_on(&mut server, 2);
    // A Portal in the level at z 5 leads into one in the caves; nothing in the caves leads out.
    let way = |world: &mut World, at: Vec3| {
        let portal = voxear_render::Portal { size: [3.0, 4.0].into(), ..voxear_render::Portal::default() };
        world.spawn((voxear_scene::Transform::new(at, Quat::IDENTITY, Vec3::ONE), portal))
    };
    let gate = way(&mut server, Vec3::new(0.0, 0.0, 5.0));
    let back = way(&mut server, Vec3::new(0.0, 10.0, 0.0));
    voxear_scene::set_dimension(&mut server, back, caves);
    server.get_mut::<voxear_render::Portal>(gate).unwrap().to = Some(back);
    let me = shared(&mut server, Vec3::new(0.0, 0.0, 4.0), NetObject::default());
    set_owner(&mut server, me, Some(PlayerId(1)));
    // Standing by the Portal, the player is named the caves it leads into, and their edits come.
    frames_in_dimensions(&mut server, &mut clients, 30, &[2]);
    let id = server.get::<Networked>(me).unwrap().id;
    let mine = entity_of(&clients[0], id).unwrap();
    let ours = voxear_scene::DimensionId(41);
    assert!(edits_in(&clients[0], ours), "named, made here and its edits in");
    // Their machine takes it through: the server takes their word, it being by the Portal.
    voxear_scene::set_dimension(&mut clients[0], mine, ours);
    clients[0].get_mut::<voxear_scene::Transform>(mine).unwrap().position = Vec3::new(0.0, 10.0, 1.0);
    frames_in_dimensions(&mut server, &mut clients, 30, &[2]);
    assert_eq!(voxear_scene::dimension_of(&server, me), caves, "taken by the Portal");
    assert_eq!(voxear_scene::dimension_of(&clients[0], mine), ours);
    // Far from any Portal, it says it went back into the level: refused, and put back in the caves.
    clients[0].get_mut::<voxear_scene::Transform>(mine).unwrap().position = Vec3::new(300.0, 10.0, 300.0);
    frames_in_dimensions(&mut server, &mut clients, 10, &[2]);
    voxear_scene::set_dimension(&mut clients[0], mine, voxear_scene::DimensionId::LEVEL);
    frames_in_dimensions(&mut server, &mut clients, 30, &[2]);
    assert_eq!(voxear_scene::dimension_of(&server, me), caves, "no Portal by it there");
    assert_eq!(voxear_scene::dimension_of(&clients[0], mine), ours, "and its machine was told where it is");
}
