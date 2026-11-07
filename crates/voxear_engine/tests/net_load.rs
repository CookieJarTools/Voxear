//! A full match: a dedicated server and 63 players, each holding a key that
//! runs their own character across the map, with 200 props moving too, all
//! in this process through a network in memory. Every player's machine has
//! every Obj, their own kept up to date every tick, and the server sends
//! each no more than the project's budget allows.
//!
//! The ignored test is the CPU benchmark: how much of one core the server
//! side of a match takes a tick, with one match and with several on the
//! same core. Run it in release:
//!
//! ```text
//! cargo test --release -p voxear_engine --test net_load -- --ignored --nocapture
//! ```

use std::net::SocketAddr;
use std::time::{Duration, Instant};

use voxear_core::math::Vec3;
use voxear_ecs::{Entity, World};
use voxear_engine::net::{self, NetId, NetObject, NetSettings, NetworkSettings, Origin, PlayerId};
use voxear_input::ActionState;
use voxear_net::{Conditions, MemoryNetwork};
use voxear_scene::Transform;

const TICK: f32 = 1.0 / 60.0;
const PLAYERS: usize = 63;
const PROPS: usize = 200;

/// A match's server, its players' machines, and the server's Objs: each
/// player's character by their id, and the props.
struct Match {
    server: World,
    clients: Vec<World>,
    characters: Vec<(PlayerId, Entity)>,
    props: Vec<Entity>,
    paces: Vec<f32>,
}

fn settings() -> NetSettings {
    NetSettings { conditions: Conditions::GOOD, ..NetSettings::for_project("Load", "1.0", &NetworkSettings::default()) }
}

impl Match {
    fn new(network: &MemoryNetwork, number: usize) -> Self {
        let mut server = World::new();
        let address: SocketAddr = format!("10.{number}.0.1:7777").parse().unwrap();
        net::serve_on(&mut server, settings(), Box::new(network.bind(address)), false).unwrap();
        let clients = (0..PLAYERS)
            .map(|n| {
                let mut world = World::new();
                let transport =
                    network.bind(format!("10.{number}.{}.{}:5000", 1 + n / 200, 1 + n % 200).parse().unwrap());
                net::connect_on(&mut world, settings(), address, Box::new(transport)).unwrap();
                world
            })
            .collect();
        let props = (0..PROPS)
            .map(|n| {
                let at = Vec3::new((n % 20) as f32 * 5.0, 0.0, (n / 20) as f32 * 5.0);
                let prop = server.spawn((Transform::from_position(at), NetObject::default()));
                net::track(&mut server, prop, Origin::Prefab("prop".into())).unwrap();
                prop
            })
            .collect();
        Self { server, clients, characters: Vec::new(), props, paces: vec![0.0; PLAYERS] }
    }

    /// A frame of the server's, and the time it took; each player joined
    /// gets a character of their own, which their held key runs.
    fn serve(&mut self) -> Duration {
        let started = Instant::now();
        let server = &mut self.server;
        net::frame_begin(server, TICK, &[]);
        for event in net::take_events(server) {
            if let net::NetEvent::PlayerJoined(player) = event {
                let at = Vec3::new(f32::from(player.0) * 2.0, 0.0, -10.0);
                let character = server.spawn((Transform::from_position(at), NetObject { priority: 2.0, range: 0.0 }));
                net::set_owner(server, character, Some(player));
                net::track(server, character, Origin::Prefab("character".into())).unwrap();
                self.characters.push((player, character));
            }
        }
        net::fixed_tick(server);
        let session = server.resource::<net::NetSession>().unwrap();
        let running: Vec<(Entity, bool)> =
            self.characters.iter().map(|&(player, entity)| (entity, session.input(player, 0).held)).collect();
        for (entity, held) in running {
            if held {
                server.get_mut::<Transform>(entity).unwrap().position.z += 0.1;
            }
        }
        for (n, prop) in self.props.iter().enumerate() {
            server.get_mut::<Transform>(*prop).unwrap().position.y =
                (n as f32 + server_tick(server) as f32 * 0.05).sin();
        }
        net::after_tick(server);
        net::frame_end(server);
        started.elapsed()
    }

    /// A frame on each player's machine, holding their key, making what
    /// the server shares.
    fn play(&mut self) {
        let held = [ActionState { held: true, ..ActionState::default() }];
        for (world, pace) in self.clients.iter_mut().zip(&mut self.paces) {
            net::frame_begin(world, TICK, &held);
            for request in net::take_spawns(world) {
                let entity = world.spawn((Transform::from_position(request.state.position),));
                net::bind(world, &request, entity);
            }
            *pace += TICK * net::time_scale(world);
            while *pace >= TICK - 1e-6 {
                net::fixed_tick(world);
                *pace -= TICK;
            }
            net::frame_end(world);
        }
    }
}

fn server_tick(world: &World) -> u64 {
    world.resource::<net::NetSession>().map_or(0, |session| session.tick())
}

#[test]
fn sixty_three_players_and_their_world_are_shared_within_the_budget() {
    let network = MemoryNetwork::new();
    let mut game = Match::new(&network, 0);
    for _ in 0..360 {
        game.serve();
        game.play();
    }
    assert_eq!(game.characters.len(), PLAYERS, "everyone joined");
    let session = game.server.resource::<net::NetSession>().unwrap();
    let stats = session.stats();
    let budget = NetworkSettings::default().kilobytes_per_second * 1024.0;
    let each = stats.sent_bytes_per_second / PLAYERS as f64;
    assert!(
        each <= budget * 1.05,
        "{:.1} kB/s to each player, over the {:.0} kB/s budget",
        each / 1024.0,
        budget / 1024.0
    );
    assert!(each >= budget * 0.3, "the budget is put to use: {:.1} kB/s", each / 1024.0);
    for client in &game.clients {
        let session = client.resource::<net::NetSession>().unwrap();
        let mine = session.local_player().expect("joined");
        let made = (1..=(PROPS + PLAYERS) as u32).filter(|id| net::entity_of(client, NetId(*id)).is_some()).count();
        assert_eq!(made, PROPS + PLAYERS, "player {} has every Obj", mine.0);
        // Their own character, sent every tick, shown a few ticks behind the server.
        let &(_, character) = game.characters.iter().find(|(player, _)| *player == mine).unwrap();
        let id = game.server.get::<net::Networked>(character).unwrap().id;
        let shown = client.get::<Transform>(net::entity_of(client, id).unwrap()).unwrap().position.z;
        let there = game.server.get::<Transform>(character).unwrap().position.z;
        assert!(
            (there - shown).abs() < 1.5,
            "player {}'s character shown at {shown}, on the server at {there}",
            mine.0
        );
    }
}

#[test]
#[ignore = "a CPU benchmark: run in release with --ignored --nocapture"]
fn matches_of_sixty_four_on_one_core() {
    for matches in [1, 2, 4, 8] {
        let network = MemoryNetwork::new();
        let mut games: Vec<Match> = (0..matches).map(|n| Match::new(&network, n + 1)).collect();
        let mut times = Vec::new();
        for frame in 0..600 {
            let mut serving = Duration::ZERO;
            for game in &mut games {
                serving += game.serve();
                game.play();
            }
            if frame >= 300 {
                times.push(serving.as_secs_f64() * 1000.0);
            }
        }
        times.sort_by(f64::total_cmp);
        let (median, worst) = (times[times.len() / 2], times[times.len() * 99 / 100]);
        let share = median / (1000.0 / 60.0) * 100.0;
        println!(
            "{matches} match{} of 64: {median:.2} ms a tick for the servers (99th: {worst:.2} ms), {share:.0}% of a core at 60 ticks a second",
            if matches == 1 { "" } else { "es" }
        );
    }
}
