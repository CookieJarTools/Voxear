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
    let mut server = World::new();
    let address: SocketAddr = "10.0.0.1:7777".parse().unwrap();
    serve_on(&mut server, settings(conditions), Box::new(network.bind(address)), false).unwrap();
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

/// Moves a ball a centimetre along x, as a `[Predicted]` behaviour would each tick.
fn roll(world: &mut World, ball: voxear_ecs::Entity) {
    if let Some(transform) = world.get_mut::<voxear_scene::Transform>(ball) {
        transform.position.x += 0.01;
    }
}

/// A client's frame with its own ball predicted: corrections first, as the engine makes them, then its ticks.
fn predicted_frame(client: &mut World) {
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
