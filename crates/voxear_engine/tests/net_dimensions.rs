//! Dimensions in a networked game, headless (no GPU, no scripts): a
//! dedicated server and two players' machines in one process, through a
//! network in memory. The server makes ground round every player, with no
//! camera of its own; a player walking into a dimension through a Portal
//! has their machine make it, with the edits the server made in it before,
//! and goes in with their Obj, while the other player, still in the level,
//! is sent nothing of it.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use voxear_core::math::Vec3;
use voxear_ecs::Entity;
use voxear_engine::{
    Engine, Game, Project,
    dimensions::{self, DimensionId, Dimensions},
    net::{self, NetSettings, NetworkSettings, PlayerId},
    physics::PhysicsScene,
};
use voxear_net::{Conditions, MemoryNetwork};
use voxear_render::Camera;
use voxear_scene::{Parent, Transform};
use voxear_terrain::{
    blocks::{
        BlockWorld,
        file::BlockFile,
        generate::{BlockEndless, BlockGenerate},
    },
    generate::Preset,
};

const FRAME: f32 = 1.0 / 60.0;

fn write(dir: &std::path::Path, files: &[(&str, String)]) {
    for (path, text) in files {
        let path = dir.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
}

fn field(height: u32) -> String {
    let mut file = BlockFile { block_size: 1.0, ..BlockFile::default() };
    file.ground.height = height;
    file.to_text()
}

/// Two walkers in a scene, each shared with players within 40 m.
fn walkers(first: Vec3, second: Vec3) -> String {
    let walker = |id: u32, name: &str, at: Vec3| {
        format!(
            "[[obj]]\nid = {id}\nname = \"{name}\"\nposition = [{}, {}, {}]\n[obj.character]\n[obj.net]\nrange = 40.0\n\n",
            at.x, at.y, at.z
        )
    };
    walker(10, "Walker 1", first) + &walker(11, "Walker 2", second)
}

fn settings() -> NetSettings {
    NetSettings {
        conditions: Conditions::GOOD,
        ..NetSettings::for_project("Gates", "1.0", &NetworkSettings::default())
    }
}

/// A game of the project in `dir`, started.
fn started(dir: &std::path::Path) -> Game {
    let mut game = Game::new(Project::open(dir).unwrap(), Engine::new()).unwrap();
    game.start(None).unwrap();
    game
}

/// A dedicated server of the project in `dir` and `players` machines joined
/// to it, each walker walking as fast as `walking` says of it, on the
/// server, and falling.
struct Match {
    server: Game,
    clients: Vec<Game>,
    walking: Arc<Mutex<HashMap<Entity, f32>>>,
}

impl Match {
    fn new(network: &MemoryNetwork, dir: &std::path::Path, players: usize) -> Self {
        let mut server = started(dir);
        let address: SocketAddr = "10.0.0.1:7777".parse().unwrap();
        net::serve_on(&mut server.engine.world, settings(), Box::new(network.bind(address)), false).unwrap();
        let walking: Arc<Mutex<HashMap<Entity, f32>>> = Arc::default();
        let walk = walking.clone();
        server.engine.schedule.add_system("Update", "walk", move |world| {
            let Some(mut physics) = world.remove_resource::<PhysicsScene>() else { return };
            for (&walker, &speed) in walk.lock().unwrap().iter() {
                let ahead = world.get::<Transform>(walker).map_or(Vec3::Z, |t| t.rotation * Vec3::Z);
                physics.move_character(world, walker, ahead * speed - Vec3::Y * 4.0, FRAME);
            }
            world.insert_resource(physics);
        });
        let clients = (0..players)
            .map(|n| {
                let mut client = started(dir);
                let transport = network.bind(format!("10.0.1.{}:5000", n + 1).parse().unwrap());
                net::connect_on(&mut client.engine.world, settings(), address, Box::new(transport)).unwrap();
                client
            })
            .collect();
        Self { server, clients, walking }
    }

    fn frames(&mut self, count: usize) {
        for _ in 0..count {
            self.server.tick(FRAME, None);
            for client in &mut self.clients {
                client.tick(FRAME, None);
            }
        }
    }

    /// Joins everyone, gives each player the walker of their number, still
    /// for now, and puts each player's camera at their own walker's eye.
    fn join(&mut self) -> Vec<Entity> {
        self.frames(60);
        let joined: Vec<PlayerId> = net::take_events(&mut self.server.engine.world)
            .into_iter()
            .filter_map(|event| match event {
                net::NetEvent::PlayerJoined(player) => Some(player),
                _ => None,
            })
            .collect();
        assert_eq!(joined.len(), self.clients.len(), "everyone joined");
        let walkers: Vec<Entity> = (1..=joined.len()).map(|n| named(&self.server, &format!("Walker {n}"))).collect();
        for (player, walker) in joined.iter().zip(&walkers) {
            net::set_owner(&mut self.server.engine.world, *walker, Some(*player));
            self.walking.lock().unwrap().insert(*walker, 0.0);
        }
        self.frames(60);
        for (n, client) in self.clients.iter_mut().enumerate() {
            let walker = named(client, &format!("Walker {}", n + 1));
            let world = &mut client.engine.world;
            world.spawn((Camera::default(), Parent(walker), Transform::from_position(Vec3::new(0.0, 1.6, 0.0))));
        }
        walkers
    }
}

/// The Obj named `name`.
fn named(game: &Game, name: &str) -> Entity {
    let world = &game.engine.world;
    let found = world.query_ref::<(Entity, &voxear_scene::Name)>().into_iter();
    found.into_iter().find(|(_, own)| own.0 == name).map(|(entity, _)| entity).unwrap_or_else(|| panic!("no {name}"))
}

fn at(game: &Game, entity: Entity) -> Vec3 {
    voxear_scene::world_matrix(&game.engine.world, entity).w_axis.truncate()
}

/// The world of blocks named `name` in `dimension`.
fn blocks_in(game: &Game, name: &str, dimension: DimensionId) -> Entity {
    let world = &game.engine.world;
    let found = world.query_ref::<(Entity, &BlockWorld, &voxear_scene::Name)>().into_iter();
    found
        .into_iter()
        .find(|(entity, _, own)| own.0 == name && dimensions::dimension_of(world, *entity) == dimension)
        .map(|(entity, ..)| entity)
        .unwrap_or_else(|| panic!("no {name} there"))
}

/// A level of a field 4 high with a Gate in it facing -z at z 40, leading
/// into the Way Back of a dimension of caves of rock 12 high, and the two
/// walkers on the field, the first 38 m short of the Gate, the second 24 m
/// off to its side.
fn gates() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let level = "format = 1\n\n[[obj]]\nid = 1\nname = \"Field\"\n[obj.block_world]\ndata = \"Blocks/Field.blockworld\"\n\n\
                 [[obj]]\nid = 2\nname = \"Gate\"\nposition = [32.0, 4.0, 40.0]\nrotation = [0.0, 180.0, 0.0]\n\
                 [obj.portal]\nsize = [3.0, 4.0]\ninto = { dimension = \"Dimensions/Caves.dimension\", portal = 2 }\n\n";
    let level = level.to_string() + &walkers(Vec3::new(32.0, 4.1, 2.0), Vec3::new(8.0, 4.1, 2.0));
    let caves = "format = 1\n\n[dimension]\nkind = \"caves\"\n\n\
                 [[obj]]\nid = 1\nname = \"Rock\"\n[obj.block_world]\ndata = \"Blocks/Rock.blockworld\"\n\n\
                 [[obj]]\nid = 2\nname = \"Way Back\"\nposition = [32.0, 12.0, 32.0]\n[obj.portal]\nsize = [3.0, 4.0]\n";
    write(
        dir.path(),
        &[
            ("voxear.toml", "[project]\nname = \"Gates\"\nstart_level = \"Levels/Main.level\"\n".into()),
            ("Levels/Main.level", "format = 1\n[[scene]]\nscene = \"Scenes/Field.scene\"\n".into()),
            ("Scenes/Field.scene", level),
            ("Dimensions/Caves.dimension", caves.into()),
            ("Blocks/Field.blockworld", field(4)),
            ("Blocks/Rock.blockworld", field(12)),
        ],
    );
    dir
}

#[test]
fn a_player_walks_into_a_dimension_with_its_edits_while_the_other_stays_in_the_level_and_sees_none_of_it() {
    let dir = gates();
    let network = MemoryNetwork::new();
    let mut game = Match::new(&network, dir.path(), 2);
    let walkers = game.join();
    // The server makes the caves and digs a pit in their rock, past the Way Back, before anyone comes.
    let caves_asset = game.server.content.parse("Dimensions/Caves.dimension").unwrap().guid().unwrap();
    let caves = game.server.make_dimension(caves_asset, 0, None).unwrap();
    let rock = blocks_in(&game.server, "Rock", caves);
    let pit: Vec<([i32; 3], u16)> =
        (30..35).flat_map(|x| (56..61).flat_map(move |z| (9..12).map(move |y| ([x, y, z], 0)))).collect();
    assert_eq!(game.server.engine.world.get_mut::<BlockWorld>(rock).unwrap().set_blocks(pit.clone()), pit.len());
    game.frames(30);
    assert!(game.clients.iter().all(|client| client.engine.world.resource::<Dimensions>().is_none()), "no one near");

    // The first walks at the Gate and through it; the server takes them into the caves.
    game.walking.lock().unwrap().insert(walkers[0], 3.0);
    let mut first_in = None;
    for frame in 0..1000 {
        game.frames(1);
        if first_in.is_none() && dimensions::dimension_of(&game.server.engine.world, walkers[0]) == caves {
            first_in = Some(frame);
        }
    }
    game.walking.lock().unwrap().insert(walkers[0], 0.0);
    game.frames(60);
    assert!(first_in.is_some(), "through the Gate on the server: at {:?}", at(&game.server, walkers[0]));
    let server = &game.server.engine.world;
    let stood = at(&game.server, walkers[0]);
    assert!(stood.z > 33.0 && (stood.y - 12.0).abs() < 0.2, "on the caves' rock past the Way Back: {stood:?}");

    // On the first player's machine: the caves made, the pit dug in them, and their walker in them.
    let first = &game.clients[0];
    let theirs = first.engine.world.resource::<Dimensions>().and_then(|made| made.find(caves_asset, 0));
    let theirs = theirs.expect("the caves made on the first player's machine");
    let their_rock = blocks_in(first, "Rock", theirs);
    let held = &first.engine.world.get::<BlockWorld>(their_rock).unwrap().blocks;
    assert!(pit.iter().all(|(at, _)| held.get(*at) == 0), "the pit the server dug is dug there");
    assert_ne!(held.get([32, 11, 38]), 0, "and the rock round it is not");
    let their_walker = named(first, "Walker 1");
    assert_eq!(dimensions::dimension_of(&first.engine.world, their_walker), theirs, "their walker went in with them");
    assert_eq!(dimensions::current(&first.engine.world), theirs, "and their camera with it");
    let shown = at(first, their_walker);
    assert!(shown.distance(stood) < 1.0, "where the server has it: {shown:?} against {stood:?}");

    // On the second's machine, in the level 24 m from where the first went: nothing of the caves.
    let second = &game.clients[1];
    assert!(second.engine.world.resource::<Dimensions>().is_none(), "the caves never made there");
    let gone = named(second, "Walker 1");
    assert!(!second.engine.world.has::<net::Networked>(gone), "the first walker no longer theirs to see");
    assert!(second.engine.world.has::<net::Networked>(named(second, "Walker 2")), "their own still is");
    assert_eq!(dimensions::dimension_of(server, walkers[1]), DimensionId::LEVEL);
    let stats = server.resource::<net::NetSession>().unwrap().stats();
    println!("a pit of {} blocks dug in the caves: {} bytes of edits sent", pit.len(), stats.edit_bytes);
    assert!(stats.edit_bytes > 0 && stats.edit_bytes < 2048, "{} bytes", stats.edit_bytes);
}

/// A level of rolling hills without end, its blocks 2 m, with the two
/// walkers 1.2 km apart, held high till ground is made under them.
fn hills() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let mut file = BlockFile { block_size: 2.0, ..BlockFile::default() };
    file.generate = Some(BlockGenerate {
        world: Some("Worlds/Hills.worldgen".into()),
        bottom: -4,
        caves: 0.0,
        endless: Some(BlockEndless { distance: 60.0, far: 0.0 }),
        ..BlockGenerate::default()
    });
    let scene =
        "format = 1\n\n[[obj]]\nid = 1\nname = \"Hills\"\n[obj.block_world]\ndata = \"Blocks/Hills.blockworld\"\n\n"
            .to_string()
            + &walkers(Vec3::new(9.0, 300.0, 9.0), Vec3::new(1209.0, 300.0, 9.0));
    write(
        dir.path(),
        &[
            ("voxear.toml", "[project]\nname = \"Gates\"\nstart_level = \"Levels/Main.level\"\n".into()),
            ("Levels/Main.level", "format = 1\n[[scene]]\nscene = \"Scenes/Hills.scene\"\n".into()),
            ("Scenes/Hills.scene", scene),
            ("Blocks/Hills.blockworld", file.to_text()),
            ("Worlds/Hills.worldgen", Preset::RollingHills.world().to_text()),
        ],
    );
    dir
}

#[test]
fn a_dedicated_server_makes_ground_round_every_player_and_stands_each_on_it() {
    let dir = hills();
    let network = MemoryNetwork::new();
    let mut game = Match::new(&network, dir.path(), 2);
    let walkers = game.join();
    // With no camera of its own, the server makes the ground round each player, however far apart.
    let made = |game: &Match, walker: Entity| {
        let place = at(&game.server, walker);
        voxear_engine::blocks::made_round(&game.server.engine.world, (place, DimensionId::LEVEL), 20.0)
    };
    for _ in 0..3000 {
        game.frames(1);
        if walkers.iter().all(|walker| made(&game, *walker) == Some(true)) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    assert!(walkers.iter().all(|walker| made(&game, *walker) == Some(true)), "ground round both");
    // Let down onto it, each stands on its own.
    for walker in &walkers {
        let ground = top(&game.server, at(&game.server, *walker));
        game.server.engine.world.get_mut::<Transform>(*walker).unwrap().position.y = ground + 2.0;
    }
    game.frames(120);
    for walker in &walkers {
        let state = game.server.engine.physics().unwrap().character_state(*walker).unwrap();
        assert!(matches!(state.ground, voxear_physics::Ground::Ground), "standing: {:?}", state.ground);
        let place = at(&game.server, *walker);
        assert!((place.y - top(&game.server, place)).abs() < 0.2, "on the ground there: {place:?}");
    }
}

/// The top of the highest block of the hills under a place, on `game`.
fn top(game: &Game, place: Vec3) -> f32 {
    let world = &game.engine.world;
    let hills = named(game, "Hills");
    let corner = voxear_scene::world_matrix(world, hills).w_axis.truncate();
    let held = world.get::<BlockWorld>(hills).unwrap();
    let column = [((place.x - corner.x) / 2.0).floor() as i32, ((place.z - corner.z) / 2.0).floor() as i32];
    let highest = (-8..200).rev().find(|&y| held.blocks.get([column[0], y, column[1]]) != 0).expect("ground there");
    corner.y + (highest + 1) as f32 * 2.0
}
