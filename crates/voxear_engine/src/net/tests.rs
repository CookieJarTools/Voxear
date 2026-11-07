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
