//! Endless Voxel Volumes in a networked game, headless (no GPU, no
//! scripts): a dedicated server and players' machines in one process,
//! through a network in memory. What the server's game changes, and what
//! a player asks of it, ends the same to the voxel on every machine, for
//! collision, rays and ways alike; felled trees and a house blown in end
//! stamped where the server's rested; a player joining late gets all of
//! it; and the bytes each change costs are measured.

use std::net::SocketAddr;
use std::time::Duration;

use voxear_core::math::Vec3;
use voxear_ecs::Entity;
use voxear_engine::{
    Engine, Game, Project,
    net::{self, NetSettings, NetworkSettings},
    voxel_collapse::all_settled,
    voxel_felling::{PieceFall, VoxelPieceBody},
    voxel_sync::{self, ShownPiece, VoxelAsk},
    voxel_volumes::{VoxelEditData, VoxelEditShape, VoxelVolumeSource},
};
use voxear_net::{Conditions, MemoryNetwork};
use voxear_render::{VoxelCut, VoxelFelled, VoxelKind, VoxelMaterial, VoxelSolids, VoxelVolume};
use voxear_scene::Name;
use voxear_terrain::blocks::plant::PlantDef;

const FRAME: f32 = 1.0 / 60.0;

/// Flat grassy ground of its own two metres up with a few oaks, its corner at the world's middle.
const SCENE: &str = "format = 1\n\n[[obj]]\nid = 1\nname = \"Ground\"\n[obj.voxel_volume]\nsize = [32.0, 32.0, 32.0]\n\
                     endless = true\nreach = 32.0\n[obj.voxel_volume.ground]\nheight = 2.0\nhills = 0.0\n\
                     [[obj.voxel_volume.trees]]\nplant = \"Plants/Oak.voxplant\"\nper_hectare = 60.0\n\n[[obj]]\nid = 2\n\
                     name = \"Eye\"\nposition = [8.0, 12.0, -6.0]\n[obj.camera]\n";

/// A project of the scene and a print of a crate, half a metre a side, with a darker band round it.
fn project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let oak = PlantDef { height: [8.0, 10.0], crown_width: [6.0, 8.0], ..PlantDef::default() };
    let files = [
        ("voxear.toml", "[project]\nname = \"Shared\"\nstart_level = \"Levels/Main.level\"\n".to_string()),
        ("Levels/Main.level", "format = 1\n[[scene]]\nscene = \"Scenes/Shared.scene\"\n".to_string()),
        ("Scenes/Shared.scene", SCENE.to_string()),
        ("Plants/Oak.voxplant", oak.to_text()),
    ];
    for (path, text) in files {
        let path = dir.path().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    let palette = vec![[150, 110, 60, 255], [90, 60, 30, 255]];
    let print = voxear_render::VoxelPrint::from_voxels(
        [8, 8, 8],
        16,
        Vec3::new(4.0, 0.0, 4.0),
        palette,
        Default::default(),
        |c| {
            if c[1] == 4 { 2 } else { 1 }
        },
    );
    std::fs::create_dir_all(dir.path().join("Prints")).unwrap();
    std::fs::write(dir.path().join("Prints/Crate.voxprint"), print.to_bytes().unwrap()).unwrap();
    dir
}

fn settings() -> NetSettings {
    NetSettings {
        conditions: Conditions::GOOD,
        ..NetSettings::for_project("Shared", "1.0", &NetworkSettings::default())
    }
}

fn started(dir: &std::path::Path) -> Game {
    let mut game = Game::new(Project::open(dir).unwrap(), Engine::new()).unwrap();
    game.start(None).unwrap();
    game
}

/// A dedicated server and its players' machines.
struct Match {
    dir: tempfile::TempDir,
    network: MemoryNetwork,
    server: Game,
    clients: Vec<Game>,
}

const SERVER: &str = "10.0.0.1:7777";

impl Match {
    fn new(players: usize) -> Self {
        let dir = project();
        let network = MemoryNetwork::new();
        let mut server = started(dir.path());
        let address: SocketAddr = SERVER.parse().unwrap();
        net::serve_on(&mut server.engine.world, settings(), Box::new(network.bind(address)), false).unwrap();
        // Felled trees are stamped back where they rest.
        voxear_engine::voxel_volumes::set_felled(&mut server.engine.world, VoxelFelled::Stamped, None);
        let mut game = Self { dir, network, server, clients: Vec::new() };
        for _ in 0..players {
            game.join();
        }
        game
    }

    /// A player's machine joins, and plays until it has everything there was.
    fn join(&mut self) {
        let mut client = started(self.dir.path());
        let transport = self.network.bind(format!("10.0.1.{}:5000", self.clients.len() + 1).parse().unwrap());
        net::connect_on(&mut client.engine.world, settings(), SERVER.parse().unwrap(), Box::new(transport)).unwrap();
        self.clients.push(client);
        for _ in 0..600 {
            self.frames(1);
            if self.clients.last().is_some_and(|client| voxel_sync::is_joined(&client.engine.world)) {
                return;
            }
        }
        panic!("a player's machine never had everything the server's volume had");
    }

    fn frames(&mut self, count: usize) {
        for _ in 0..count {
            // While the server's volume works out what gives way on a thread, the clock is let catch up.
            if !all_settled(&self.server.engine.world) {
                std::thread::sleep(Duration::from_millis(1000 / 60));
            }
            self.server.tick(FRAME, None);
            for client in &mut self.clients {
                client.tick(FRAME, None);
            }
        }
    }

    /// Plays until the server has nothing settling or falling and every player's machine has caught up: how
    /// many frames it took, and the most pieces any player's machine showed at once.
    fn play_until_still(&mut self, most: usize) -> (usize, usize) {
        let mut shown = 0;
        for frame in 0..most {
            self.frames(1);
            let counts: Vec<usize> =
                self.clients.iter().map(|c| c.engine.world.query_ref::<&ShownPiece>().into_iter().count()).collect();
            shown = shown.max(counts.iter().copied().max().unwrap_or(0));
            let falling = self.server.engine.world.query_ref::<&VoxelPieceBody>().into_iter().count();
            if frame > 30 && falling == 0 && all_settled(&self.server.engine.world) && counts.iter().all(|&c| c == 0) {
                // A few frames more for the last news to arrive.
                self.frames(20);
                return (frame, shown);
            }
        }
        panic!("still falling after {most} frames");
    }

    fn sent(&self) -> net::VoxelBytes {
        net::voxel_bytes(&self.server.engine.world)
    }
}

fn ground(game: &Game) -> Entity {
    game.engine.world.query_ref::<(Entity, &Name)>().into_iter().find(|(_, n)| n.0 == "Ground").unwrap().0
}

fn volume(game: &Game) -> VoxelVolume {
    game.engine.world.get::<VoxelVolume>(ground(game)).unwrap().clone()
}

/// Which voxels of the box from `low` to `high` metres from the volume's corner are solid, as collision finds
/// them.
fn solids(volume: &VoxelVolume, low: Vec3, high: Vec3) -> Vec<bool> {
    let (low, high) = ((low * 16.0).as_ivec3(), (high * 16.0).as_ivec3());
    let size = high - low;
    let mut solid = vec![false; (size.x * size.y * size.z) as usize];
    VoxelSolids::of(volume).unwrap().fill(low.to_array(), size.to_array(), &mut solid);
    solid
}

/// Where rays straight down and slanting meet the volume, on a grid over the box from `low` to `high`.
fn rays(volume: &VoxelVolume, low: Vec3, high: Vec3) -> Vec<Option<[i32; 3]>> {
    let mut hits = Vec::new();
    for x in 0..24 {
        for z in 0..24 {
            let at = low + Vec3::new(x as f32 + 0.37, 0.0, z as f32 + 0.61) * (high - low) / 24.0;
            for way in [Vec3::NEG_Y, Vec3::new(0.4, -1.0, 0.25).normalize()] {
                let hit = voxear_render::voxel_raycast(volume, Vec3::ZERO, Vec3::new(at.x, high.y, at.z), way, 40.0);
                hits.push(hit.map(|hit| (hit.point * 16.0).floor().as_ivec3().to_array()));
            }
        }
    }
    hits
}

/// Checks every player's machine has the server's volume, to the voxel: its edits, its changes, what collision
/// and rays meet over the box from `low` to `high` metres, and no piece still shown.
fn agree(game: &Match, low: Vec3, high: Vec3, what: &str) {
    let theirs = volume(&game.server);
    let source = |g: &Game| g.engine.world.get::<VoxelVolumeSource>(ground(g)).unwrap().0.edits.clone();
    let (solid, hits) = (solids(&theirs, low, high), rays(&theirs, low, high));
    for (n, client) in game.clients.iter().enumerate() {
        let ours = volume(client);
        assert_eq!(source(client), source(&game.server), "{what}: player {n}'s edits");
        assert_eq!(ours.edits.len(), theirs.edits.len(), "{what}: player {n}'s edits made");
        let off = ours.changes.delta_from(&theirs.changes);
        assert!(off.is_empty(), "{what}: player {n}'s changes differ by {} voxels: {off:?}", off.voxels());
        let differ = solids(&ours, low, high).iter().zip(&solid).filter(|(a, b)| a != b).count();
        assert_eq!(differ, 0, "{what}: player {n}'s collision differs in {differ} voxels");
        assert_eq!(rays(&ours, low, high), hits, "{what}: player {n}'s rays");
        let shown = client.engine.world.query_ref::<&ShownPiece>().into_iter().count();
        assert_eq!(shown, 0, "{what}: player {n} still shows a piece");
        assert_eq!(voxel_sync::predicted(&client.engine.world), 0, "{what}: player {n} still waits for an answer");
    }
}

/// The way found from `from` to `to` over the volume on a machine, once the cells round a walker there are made.
fn way(game: &mut Game, from: Vec3, to: Vec3) -> Vec<[i32; 3]> {
    let world = &mut game.engine.world;
    let walker = world.spawn((voxear_scene::Transform::from_position(from),));
    world.insert(walker, voxear_engine::nav_agent::NavAgent::default());
    voxear_engine::voxel_nav::ensure(world);
    for _ in 0..4 {
        voxear_engine::voxel_nav::update(world);
        voxear_engine::voxel_nav::settle(world, Duration::from_secs(60));
    }
    let dimension = voxear_scene::DimensionId::LEVEL;
    let found = voxear_engine::voxel_ways::finder(world)
        .way(world, dimension, from, to, &voxear_engine::voxel_ways::VoxelWalker::of_size(1.8, 0.35))
        .expect("a way");
    world.despawn(walker);
    found.corners.iter().map(|c| (*c * 16.0).round().as_ivec3().to_array()).collect()
}

fn points(low: Vec3, high: Vec3) -> Vec<Vec3> {
    let mut points = Vec::new();
    let (low, high) = ((low * 16.0).as_ivec3(), (high * 16.0).as_ivec3());
    for x in low.x..high.x {
        for y in low.y..high.y {
            for z in low.z..high.z {
                points.push((Vec3::new(x as f32, y as f32, z as f32) + 0.5) / 16.0);
            }
        }
    }
    points
}

/// The bytes of news sent to each player since `before`.
fn news_since(game: &Match, before: net::VoxelBytes) -> u64 {
    (game.sent().news - before.news) / game.clients.len() as u64
}

#[test]
fn a_dig_a_print_and_a_cut_agree_to_the_voxel_on_every_machine() {
    let mut game = Match::new(2);
    let (low, high) = (Vec3::new(0.0, 0.0, 0.0), Vec3::new(20.0, 6.0, 20.0));
    agree(&game, low, high, "as joined");
    let mut costs = Vec::new();

    // The server's own wall of bricks, a shaped edit.
    let before = game.sent();
    let wall = VoxelEditData {
        shape: VoxelEditShape::Box,
        at: [8.0, 2.0, 10.0],
        size: [0.5, 2.5, 8.0],
        rotation: [0.0, 20.0, 0.0],
        fill: Some(VoxelMaterial::Bricks),
        tint: Some([180, 90, 70]),
        ..VoxelEditData::default()
    };
    assert_eq!(voxear_engine::voxel_volumes::shape_edit(&mut game.server.engine.world, &wall, None), 1);
    game.frames(10);
    costs.push(("a turned box of bricks, the server's", news_since(&game, before)));
    agree(&game, low, high, "a wall");

    // A player digs a hole a voxel at a time: a request the server makes.
    let before = game.sent();
    let hole = points(Vec3::new(4.0, 1.5, 4.0), Vec3::new(4.5, 2.0, 4.5));
    let ask = VoxelAsk::Set { points: hole.clone(), to: None };
    assert!(voxel_sync::ask(&mut game.clients[0].engine.world, &ask).is_some());
    // Shown dug on the player's machine at once, until the server's answer.
    assert!(!volume(&game.clients[0]).changes.no_voxels(), "shown dug at once");
    assert_eq!(voxel_sync::predicted(&game.clients[0].engine.world), 1);
    game.frames(10);
    costs.push(("512 voxels dug, a player's", news_since(&game, before)));
    assert!(!volume(&game.server).changes.no_voxels(), "dug on the server");
    agree(&game, low, high, "a hole dug");

    // One voxel placed, one dug.
    let before = game.sent();
    let brick = VoxelKind { material: VoxelMaterial::Planks, tint: None };
    voxel_sync::ask(
        &mut game.clients[1].engine.world,
        &VoxelAsk::Set { points: vec![Vec3::new(6.03, 2.03, 6.03)], to: Some(brick) },
    );
    game.frames(10);
    costs.push(("a voxel placed", news_since(&game, before)));
    let before = game.sent();
    voxel_sync::ask(
        &mut game.clients[1].engine.world,
        &VoxelAsk::Set { points: vec![Vec3::new(7.03, 1.97, 7.03)], to: None },
    );
    game.frames(10);
    costs.push(("a voxel dug", news_since(&game, before)));
    agree(&game, low, high, "a voxel placed and one dug");

    // Half the hole put back as the ground made it.
    let before = game.sent();
    voxel_sync::ask(&mut game.clients[1].engine.world, &VoxelAsk::Restore { points: hole[..256].to_vec() });
    game.frames(10);
    costs.push(("256 voxels restored", news_since(&game, before)));
    agree(&game, low, high, "voxels restored");

    // A print of a crate, fitted to the ground, asked for by a player.
    let before = game.sent();
    let crate_ = VoxelEditData {
        fit: true,
        fill: Some(VoxelMaterial::Gravel),
        ..VoxelEditData::printed("Prints/Crate.voxprint".into(), [12.0, 2.0, 12.0], [0.0, 30.0, 0.0])
    };
    let print = match game.clients[0].content.voxel_print("Prints/Crate.voxprint") {
        Some((guid, _)) => VoxelEditData { print: Some(guid.to_string()), ..crate_ },
        None => panic!("the print is in the project"),
    };
    voxel_sync::ask(&mut game.clients[0].engine.world, &VoxelAsk::Print(print));
    game.frames(10);
    costs.push(("a print, fitted", news_since(&game, before)));
    assert_eq!(volume(&game.server).edits.len(), 2, "printed on the server");
    agree(&game, low, high, "a print");

    // Grass cut in a swing, asked for by a player.
    let before = game.sent();
    let cut = VoxelCut::Ball { at: Vec3::new(15.0, 2.2, 5.0), radius: 1.5 };
    let ask = VoxelAsk::Cut { cut, toward: None, pieces: voxear_engine::voxel_felling::CutPieces::Fall, rest: None };
    voxel_sync::ask(&mut game.clients[1].engine.world, &ask);
    game.frames(10);
    costs.push(("grass cut, 3 m across", news_since(&game, before)));
    assert!(volume(&game.server).changes.any_cut(), "cut on the server");
    agree(&game, low, high, "grass cut");

    // Ways round the wall agree, from the same voxels.
    let (from, to) = (Vec3::new(5.0, 2.1, 10.0), Vec3::new(11.0, 2.1, 10.0));
    let theirs = way(&mut game.server, from, to);
    assert!(theirs.len() > 2, "the server's way goes round the wall: {theirs:?}");
    for client in &mut game.clients {
        assert_eq!(way(client, from, to), theirs, "a player's way round the wall");
    }

    let asks = game.sent().asks;
    for (what, bytes) in &costs {
        eprintln!("{what}: {bytes} bytes to each player");
    }
    eprintln!("requests heard: {asks} bytes in all");
    for (what, bytes) in &costs {
        assert!(*bytes < 600, "{what} cost {bytes} bytes");
    }
}

#[test]
fn a_felled_tree_and_a_blown_in_house_end_stamped_the_same_on_every_machine() {
    let mut game = Match::new(2);
    let (low, high) = (Vec3::new(-4.0, 0.0, -4.0), Vec3::new(28.0, 14.0, 28.0));

    // A tree felled, by a player's cut through its trunk.
    let trunks =
        voxear_render::voxel_tree_trunks(&volume(&game.server), Vec3::new(2.0, 0.0, 2.0), Vec3::new(26.0, 0.0, 26.0));
    assert!(!trunks.is_empty(), "trees grow there");
    let before = game.sent();
    let mut felled = false;
    for trunk in trunks {
        let cut = VoxelCut::Slice { at: trunk + Vec3::Y, across: Vec3::new(0.2, 1.0, 0.0), radius: 0.9 };
        let ask = VoxelAsk::Cut {
            cut,
            toward: Some(Vec3::X),
            pieces: voxear_engine::voxel_felling::CutPieces::Fall,
            rest: None,
        };
        voxel_sync::ask(&mut game.clients[0].engine.world, &ask);
        game.frames(4);
        if game.server.engine.world.query_ref::<&VoxelPieceBody>().into_iter().count() > 0 {
            felled = true;
            break;
        }
    }
    assert!(felled, "a cut took a tree loose");
    let (frames, shown) = game.play_until_still(60 * 40);
    let tree_bytes = news_since(&game, before);
    eprintln!(
        "a tree felled and stamped where it rested in {frames} frames: {tree_bytes} bytes of news to each player, \
         {} of snapshots in all, {shown} pieces shown at most",
        game.sent().poses - before.poses
    );
    assert!(shown >= 1, "players saw it fall");
    let stamped = volume(&game.server).changes.count();
    assert!(stamped > 0, "stamped back into the volume");
    agree(&game, low, high, "a tree felled");

    // A house of bricks with a roof of planks, the server's, its corner blown out by a player.
    let boxed = |at: [f32; 3], size: [f32; 3], fill| VoxelEditData {
        shape: VoxelEditShape::Box,
        at,
        size,
        fill,
        ..VoxelEditData::default()
    };
    let house = [
        boxed([20.5, 2.0, 20.0], [5.0, 2.5, 4.0], Some(VoxelMaterial::Bricks)),
        boxed([20.5, 2.0, 20.0], [4.5, 2.5, 3.5], None),
        boxed([21.5, 2.0, 18.0], [1.0, 2.0, 0.6], None),
        boxed([20.5, 4.5, 20.0], [5.5, 0.25, 4.5], Some(VoxelMaterial::Planks)),
    ];
    for edit in &house {
        voxear_engine::voxel_volumes::shape_edit(&mut game.server.engine.world, edit, None);
    }
    game.frames(10);
    agree(&game, low, high, "a house built");
    let before = game.sent();
    let blast = VoxelAsk::Blast {
        at: Vec3::new(18.0, 3.5, 18.0),
        radius: 1.8,
        pieces: voxear_engine::voxel_felling::CutPieces::Fall,
        throw: 0.0,
    };
    voxel_sync::ask(&mut game.clients[1].engine.world, &blast);
    let mut fell = 0;
    for _ in 0..60 * 3 {
        game.frames(1);
        let built = game
            .server
            .engine
            .world
            .query_ref::<&VoxelPieceBody>()
            .into_iter()
            .filter(|b| b.built && b.fall != PieceFall::Resting)
            .count();
        fell = fell.max(built);
    }
    let (frames, shown) = game.play_until_still(60 * 60);
    let house_bytes = news_since(&game, before);
    let poses = game.sent().poses - before.poses;
    eprintln!(
        "a house's corner blown in: {fell} pieces falling at most, still {frames} frames after; {house_bytes} bytes \
         of news to each player, {poses} of snapshots in all, {shown} pieces shown at most"
    );
    assert!(fell >= 1 && shown >= 1, "what was built came loose and players saw it fall");
    agree(&game, low, high, "a house blown in");
}

#[test]
fn a_player_joining_late_gets_every_change_made_before() {
    let mut game = Match::new(1);
    let (low, high) = (Vec3::new(0.0, 0.0, 0.0), Vec3::new(24.0, 8.0, 24.0));
    let server = &mut game.server.engine.world;
    // A street of cobbles, a cellar dug a voxel at a time, a tower placed a voxel at a time, cut grass.
    let street = VoxelEditData::area(
        vec![[1.0, 1.0], [22.0, 1.0], [22.0, 4.0], [1.0, 4.0]],
        1.5,
        2.0,
        Some(VoxelMaterial::Cobbles),
    );
    voxear_engine::voxel_volumes::shape_edit(server, &street, None);
    voxear_engine::voxel_volumes::set_voxels(
        server,
        &points(Vec3::new(14.0, 0.5, 14.0), Vec3::new(17.0, 2.0, 17.0)),
        None,
        None,
    );
    let stone = VoxelKind { material: VoxelMaterial::Ashlar, tint: Some([200, 190, 170]) };
    voxear_engine::voxel_volumes::set_voxels(
        server,
        &points(Vec3::new(6.0, 2.0, 16.0), Vec3::new(7.0, 6.0, 17.0)),
        Some(stone),
        None,
    );
    let cut = VoxelCut::Ball { at: Vec3::new(10.0, 2.2, 12.0), radius: 2.0 };
    voxear_engine::voxel_felling::cut(server, &cut, Default::default(), None);
    game.frames(20);
    agree(&game, low, high, "the first player");
    // A second player, joining now.
    let before = game.sent();
    game.join();
    let joining = game.sent().joining - before.joining;
    let regions: usize = volume(&game.server).changes.regions().iter().map(|(_, bytes)| bytes.len()).sum();
    eprintln!("a player joining late was sent {joining} bytes; the volume's region files hold {regions}");
    assert!(joining < regions as u64 + 1024, "about what the region files hold: {joining} for {regions}");
    agree(&game, low, high, "the player joining late");
    // And what changes from there on reaches both.
    voxear_engine::voxel_volumes::edit(&mut game.server.engine.world, Vec3::new(18.0, 2.0, 8.0), 1.2, None, None);
    game.frames(10);
    agree(&game, low, high, "after joining");
}

/// The voxel `at` (metres from the volume's corner) as a machine's changes set it.
fn set_at(game: &Game, at: Vec3) -> Option<voxear_render::VoxelHeld> {
    volume(game).changes.get((at * 16.0).floor().as_ivec3())
}

#[test]
fn a_players_own_dig_and_box_show_at_once_and_stay_as_the_server_answers_with_nothing_twice() {
    let mut game = Match::new(2);
    let (low, high) = (Vec3::new(0.0, 0.0, 0.0), Vec3::new(20.0, 6.0, 20.0));
    // A dig: shown at once, and on every frame after, never gone and back.
    let cell = Vec3::new(5.03, 1.97, 5.03);
    voxel_sync::ask(&mut game.clients[0].engine.world, &VoxelAsk::Set { points: vec![cell], to: None }).unwrap();
    assert_eq!(set_at(&game.clients[0], cell), Some(voxear_render::VoxelHeld::Air), "dug at once");
    // A box of bricks: one edit at once, one edit once answered, never two.
    let wall = VoxelEditData {
        shape: VoxelEditShape::Box,
        at: [9.0, 2.0, 9.0],
        size: [2.0, 1.5, 0.5],
        fill: Some(VoxelMaterial::Bricks),
        ..VoxelEditData::default()
    };
    voxel_sync::ask(&mut game.clients[0].engine.world, &VoxelAsk::Shape(wall)).unwrap();
    assert_eq!(volume(&game.clients[0]).edits.len(), 1, "built at once");
    let mut frames = 0;
    while voxel_sync::predicted(&game.clients[0].engine.world) > 0 {
        game.frames(1);
        frames += 1;
        assert!(frames < 120, "answered");
        assert_eq!(set_at(&game.clients[0], cell), Some(voxear_render::VoxelHeld::Air), "still dug, frame {frames}");
        assert_eq!(volume(&game.clients[0]).edits.len(), 1, "one wall, frame {frames}");
    }
    eprintln!("answered in {frames} frames");
    for _ in 0..10 {
        game.frames(1);
        assert_eq!(volume(&game.clients[0]).edits.len(), 1, "one wall after the answer");
    }
    agree(&game, low, high, "dug and built at once");

    // Refused by the rules: shown at once, then let go.
    let rules = voxear_engine::voxel_asks::VoxelAskRules {
        allowed: !voxear_engine::voxel_asks::VoxelAskKind::Place.bit(),
        ..Default::default()
    };
    voxear_engine::voxel_asks::set_rules(&mut game.server.engine.world, rules);
    let placed = Vec3::new(7.03, 2.03, 7.03);
    let planks = Some(VoxelKind { material: VoxelMaterial::Planks, tint: None });
    voxel_sync::ask(&mut game.clients[1].engine.world, &VoxelAsk::Set { points: vec![placed], to: planks }).unwrap();
    assert!(set_at(&game.clients[1], placed).is_some(), "placed at once");
    game.frames(20);
    assert_eq!(set_at(&game.clients[1], placed), None, "let go once refused");
    let refusals = voxear_engine::voxel_asks::refusals(&game.server.engine.world);
    assert_eq!(refusals.last().map(|r| r.reason.as_str()), Some("players may not place"), "{refusals:?}");
    agree(&game, low, high, "a place refused");

    // Refused by the game's own check in Rust: no blasts.
    voxear_engine::voxel_asks::set_ask_check(
        &mut game.server.engine.world,
        std::sync::Arc::new(|_, _, ask| !matches!(ask, VoxelAsk::Blast { .. })),
    );
    let blast = VoxelAsk::Blast {
        at: Vec3::new(9.0, 2.5, 9.0),
        radius: 1.0,
        pieces: voxear_engine::voxel_felling::CutPieces::Fall,
        throw: 0.0,
    };
    voxel_sync::ask(&mut game.clients[1].engine.world, &blast).unwrap();
    game.frames(20);
    assert_eq!(voxear_engine::voxel_asks::refusals(&game.server.engine.world).len(), 2);
    agree(&game, low, high, "a blast refused");

    // Checked as a C# check does, within the frame each came in: digs deeper than half a metre under the ground
    // refused with a reason.
    game.server.engine.schedule.add_system("Update", "check", |world| {
        voxear_engine::voxel_asks::set_scripts_check(world, true);
        while let Some((id, _, info)) = voxear_engine::voxel_asks::next_for_script(world) {
            let deep = info.at.y < 1.5;
            voxear_engine::voxel_asks::answer_from_script(world, id, !deep, if deep { "too deep" } else { "" });
        }
    });
    game.frames(2);
    let deep = Vec3::new(3.03, 1.03, 3.03);
    voxel_sync::ask(&mut game.clients[0].engine.world, &VoxelAsk::Set { points: vec![deep], to: None }).unwrap();
    let shallow = Vec3::new(3.03, 1.97, 3.03);
    voxel_sync::ask(&mut game.clients[0].engine.world, &VoxelAsk::Set { points: vec![shallow], to: None }).unwrap();
    game.frames(20);
    assert_eq!(set_at(&game.server, deep), None, "the deep dig refused");
    assert_eq!(set_at(&game.server, shallow), Some(voxear_render::VoxelHeld::Air), "the shallow dig made");
    let refusals = voxear_engine::voxel_asks::refusals(&game.server.engine.world);
    assert_eq!(refusals.last().map(|r| r.reason.as_str()), Some("too deep"));
    agree(&game, low, high, "checked as C# checks");
    // What the stats say.
    let players = net::voxel_players(&game.server.engine.world);
    assert_eq!(players.len(), 2);
    assert!(players.iter().all(|p| p.joined && p.news_bytes > 0 && p.news_messages > 0), "{players:?}");
    assert!(voxear_engine::voxel_asks::waiting(&game.server.engine.world).is_empty());
}

fn ropes(game: &Game) -> Vec<(Entity, voxear_engine::voxel_ropes::RopeHolds)> {
    use voxear_engine::voxel_ropes::{RopeHolds, VoxelRope};
    let world = &game.engine.world;
    let found: Vec<Entity> = world.query_ref::<(Entity, &VoxelRope)>().into_iter().map(|(e, _)| e).collect();
    found.into_iter().map(|e| (e, world.get::<RopeHolds>(e).copied().unwrap_or_default())).collect()
}

#[test]
fn a_rope_tied_and_cut_by_players_hangs_and_falls_alike_on_every_machine_and_a_late_joiner_sees_it() {
    let mut game = Match::new(2);
    let (low, high) = (Vec3::new(12.0, 0.0, 0.0), Vec3::new(28.0, 9.0, 8.0));
    // A box of planks built in the air, a player's rope tied over it.
    let crate_ = VoxelEditData {
        shape: VoxelEditShape::Box,
        at: [20.0, 4.0, 4.0],
        size: [1.0, 1.0, 1.0],
        fill: Some(VoxelMaterial::Planks),
        ..VoxelEditData::default()
    };
    voxear_engine::voxel_volumes::shape_edit(&mut game.server.engine.world, &crate_, None);
    game.frames(5);
    let tied = Vec3::new(20.5, 8.0, 4.0);
    let rope = voxel_sync::hang(&mut game.clients[0].engine.world, tied, Vec3::new(20.0, 4.9, 4.0), 0.0, 0.0).unwrap();
    assert_eq!(ropes(&game.clients[0]).len(), 1, "shown at once");
    // It hangs on the server; every player's machine shows the piece held by one rope.
    let mut hung = false;
    for _ in 0..60 * 4 {
        game.frames(1);
        let held = |g: &Game| {
            ropes(g).iter().any(|(_, holds)| holds.piece.is_some_and(|p| g.engine.world.has::<ShownPiece>(p)))
        };
        if ropes(&game.server).iter().any(|(_, h)| h.piece.is_some()) && game.clients.iter().all(held) {
            hung = true;
            break;
        }
    }
    assert!(hung, "it hangs from the rope on every machine");
    for client in &game.clients {
        assert_eq!(ropes(client).len(), 1, "one rope, not two");
    }
    assert!(game.clients[0].engine.world.is_alive(rope), "the player's own rope became the server's");
    game.frames(60);
    // A player joining now sees it hanging.
    game.join();
    game.frames(10);
    let late = &game.clients[2];
    let held = ropes(late);
    assert_eq!(held.len(), 1);
    assert!(held[0].1.piece.is_some_and(|p| late.engine.world.has::<ShownPiece>(p)), "the late joiner's rope holds it");
    // The other player cuts it: it falls, rests and is stamped the same everywhere.
    let theirs = ropes(&game.clients[1])[0].0;
    assert!(voxel_sync::cut_rope(&mut game.clients[1].engine.world, theirs));
    game.play_until_still(60 * 30);
    for g in std::iter::once(&game.server).chain(&game.clients) {
        assert!(ropes(g).iter().all(|(_, holds)| holds.snapped), "snapped everywhere");
    }
    agree(&game, low, high, "a rope cut");
}
