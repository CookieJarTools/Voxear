//! What grows and walks on an endless Voxel Volume made by a starter
//! generator, in a game: its trees and cover keep to the World Areas of its
//! level, and its mobs spawn on its ground round the camera, within the
//! areas they keep to, walk it, and go once the camera is far (headless: no
//! GPU, no scripts).

use voxear_assets::AssetGuid;
use voxear_core::math::Vec3;
use voxear_ecs::Entity;
use voxear_engine::{
    Engine, Game, Project,
    block_mob::BlockMob,
    voxel_mobs::{VoxelMobSpawned, VoxelMobsData},
    voxel_volumes::{VoxelCoverData, VoxelVolumeSource},
};
use voxear_render::{VoxelCoverKind, VoxelMaterial, VoxelVolume};
use voxear_scene::{Name, Transform};

/// A Rolling Hills volume with its corner at the world's middle, a camera over it, and a World Area west of X 0.
const SCENE: &str = "format = 1\n\n[[obj]]\nid = 1\nname = \"Ground\"\n[obj.voxel_volume]\nsize = [16.0, 64.0, 16.0]\n\
                     endless = true\nreach = 48.0\ngenerator = \"Worlds/Hills.worldgen\"\n\n[[obj]]\nid = 2\n\
                     name = \"Eye\"\nposition = [0.0, 60.0, 0.0]\n[obj.camera]\n\n[[obj]]\nid = 3\nname = \"Pasture\"\n\
                     [obj.world_area]\noutline = [[-400.0, -400.0], [0.0, -400.0], [0.0, 400.0], [-400.0, 400.0]]\n";

const SHEEP: &str = "format = 1\n\n[[obj]]\nid = 1\nname = \"Sheep\"\n[obj.character]\n[obj.block_mob]\nwander = 6.0\n";

fn game() -> (tempfile::TempDir, Game) {
    let dir = tempfile::tempdir().unwrap();
    let files = [
        ("voxear.toml", "[project]\nname = \"Flock\"\nstart_level = \"Levels/Main.level\"\n".to_string()),
        ("Levels/Main.level", "format = 1\n[[scene]]\nscene = \"Scenes/Hills.scene\"\n".to_string()),
        ("Scenes/Hills.scene", SCENE.to_string()),
        ("Worlds/Hills.worldgen", voxear_terrain::generate::Preset::RollingHills.world().to_text()),
        ("Mobs/Sheep.prefab", SHEEP.to_string()),
    ];
    for (path, text) in files {
        let path = dir.path().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    let mut game = Game::new(Project::open(dir.path()).unwrap(), Engine::new()).unwrap();
    game.start(None).unwrap();
    (dir, game)
}

fn named(game: &Game, name: &str) -> Entity {
    game.engine.world.query_ref::<(Entity, &Name)>().into_iter().find(|(_, n)| n.0 == name).unwrap().0
}

/// Gives the Pasture area the tag `tag`, and the volume `change`d.
fn rule(game: &mut Game, tag: AssetGuid, change: impl FnOnce(&mut voxear_engine::voxel_volumes::VoxelVolumeData)) {
    let pasture = named(game, "Pasture");
    let area = game.engine.world.get_mut::<voxear_engine::world_areas::WorldArea>(pasture).unwrap();
    area.tags = vec![voxear_engine::world_areas::AreaEntry { tag: Some(tag), ..Default::default() }];
    let ground = named(game, "Ground");
    let mut data = game.engine.world.get::<VoxelVolumeSource>(ground).unwrap().0.clone();
    change(&mut data);
    voxear_engine::voxel_volumes::set(&mut game.content, &mut game.engine.world, ground, data);
}

fn spawned(game: &Game) -> Vec<(Entity, Vec3)> {
    game.engine
        .world
        .query_ref::<(Entity, &VoxelMobSpawned, &Transform)>()
        .into_iter()
        .map(|(entity, _, place)| (entity, place.position))
        .collect()
}

#[test]
fn trees_and_crops_kept_to_an_area_are_given_its_outline_from_the_volumes_corner() {
    let (_dir, mut game) = game();
    let tag = AssetGuid::from_u128(0x5A57_0000);
    rule(&mut game, tag, |data| {
        let mut wheat = VoxelCoverData::of(VoxelCoverKind::Wheat);
        wheat.within = vec![tag];
        data.cover = vec![wheat];
    });
    // The volume's Obj moved: the area is laid out from its corner again.
    let ground = named(&game, "Ground");
    game.engine.world.get_mut::<Transform>(ground).unwrap().position = Vec3::new(10.0, 0.0, -5.0);
    game.tick(1.0 / 60.0, None);
    let volume = game.engine.world.get::<VoxelVolume>(ground).unwrap();
    assert_eq!(volume.areas.len(), 1, "the pasture reaches the volume");
    assert_eq!(volume.areas[0].tags, vec![tag.to_u128()]);
    assert_eq!(volume.areas[0].outline[1], [-10.0, -395.0], "from the volume's corner");
    assert_eq!(volume.cover[0].areas.within, vec![tag.to_u128()]);
    // Set again from its data, it keeps its areas until they are found again.
    let data = game.engine.world.get::<VoxelVolumeSource>(ground).unwrap().0.clone();
    voxear_engine::voxel_volumes::set(&mut game.content, &mut game.engine.world, ground, data);
    assert_eq!(game.engine.world.get::<VoxelVolume>(ground).unwrap().areas.len(), 1);
    // Written and read back as it was.
    let written = toml::to_string(&game.engine.world.get::<VoxelVolumeSource>(ground).unwrap().0).unwrap();
    assert!(written.contains("kind = \"wheat\"") && written.contains("within"), "{written}");
}

#[test]
fn mobs_spawn_on_a_volumes_ground_round_the_camera_within_their_area_and_wander_it() {
    let (_dir, mut game) = game();
    let sheep = game.content.db().guid_of("Mobs/Sheep.prefab").unwrap();
    let tag = AssetGuid::from_u128(0x5A57_0001);
    rule(&mut game, tag, |data| {
        let flock = VoxelMobsData {
            prefab: Some(sheep),
            per_hectare: 40.0,
            most: 5,
            near: 8.0,
            far: 30.0,
            on: vec![VoxelMaterial::Grass],
            within: vec![tag],
            ..VoxelMobsData::default()
        };
        data.mobs = vec![flock];
    });
    let started = std::time::Instant::now();
    for _ in 0..240 {
        game.tick(1.0 / 60.0, None);
    }
    let took = started.elapsed();
    let ground = named(&game, "Ground");
    let volume = game.engine.world.get::<VoxelVolume>(ground).unwrap().clone();
    let corner = game.engine.world.get::<Transform>(ground).unwrap().position;
    let flock = spawned(&game);
    eprintln!("{} spawned in {took:?} of 4 s", flock.len());
    assert_eq!(flock.len(), 5, "as many as it may have");
    for &(sheep, at) in &flock {
        let flat = Vec3::new(at.x, 0.0, at.z).length();
        // Spawned between near and far, and wandered a little since.
        assert!((2.0..=37.0).contains(&flat), "between near and far of the camera: {at}");
        assert!(at.x < 6.0, "in the pasture, west of X 0: {at}");
        let (top, material) = voxear_render::voxel_ground_at(&volume, at.x - corner.x, at.z - corner.z).unwrap();
        assert_eq!(material, VoxelMaterial::Grass);
        assert!((at.y - corner.y - top).abs() < 1.5, "on the ground: at {at}, its top {top}");
        assert!(game.engine.world.get::<BlockMob>(sheep).is_some(), "a Block Mob to walk");
    }
    // They wander about where they were spawned, on the ground.
    let before: Vec<Vec3> = flock.iter().map(|(_, at)| *at).collect();
    for _ in 0..600 {
        game.tick(1.0 / 60.0, None);
    }
    let after = spawned(&game);
    let moved = after.iter().zip(&before).filter(|((_, now), was)| now.distance(**was) > 0.5).count();
    eprintln!("{moved} of {} moved", after.len());
    assert!(moved >= 2, "they wander: {moved} moved");
    for &(_, at) in &after {
        let (top, _) = voxear_render::voxel_ground_at(&volume, at.x - corner.x, at.z - corner.z).unwrap();
        assert!((at.y - corner.y - top).abs() < 1.5, "still on the ground: at {at}, its top {top}");
    }
    // The camera gone far away: they go, and others come round it.
    let eye = named(&game, "Eye");
    game.engine.world.get_mut::<Transform>(eye).unwrap().position = Vec3::new(-600.0, 80.0, 0.0);
    for _ in 0..120 {
        game.tick(1.0 / 60.0, None);
    }
    let now = spawned(&game);
    assert!(now.iter().all(|(_, at)| at.x < -500.0), "those left behind are gone: {now:?}");
}

/// Ways that go by a corner of their own, counting how often they are asked.
struct ByCorner(std::sync::atomic::AtomicUsize);

impl voxear_engine::voxel_ways::VoxelWays for ByCorner {
    fn way(
        &self,
        _: &voxear_ecs::World,
        _: voxear_scene::DimensionId,
        from: Vec3,
        to: Vec3,
        _: f32,
        _: f32,
    ) -> Option<voxear_engine::voxel_ways::VoxelWay> {
        self.0.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let corner = Vec3::new(from.x, to.y, to.z);
        Some(voxear_engine::voxel_ways::VoxelWay { corners: vec![from, corner, to], complete: true })
    }
}

#[test]
fn a_mob_on_a_volume_walks_the_ways_a_navigation_set_in_the_world_finds() {
    let (dir, mut game) = game();
    let ways = std::sync::Arc::new(ByCorner(std::sync::atomic::AtomicUsize::new(0)));
    voxear_engine::voxel_ways::set(&mut game.engine.world, ways.clone());
    let placed = voxear_scene::Placement { position: Vec3::new(-10.0, 50.0, 0.0), rotation: Default::default() };
    let _ = dir;
    let sheep = game.instantiate("Mobs/Sheep.prefab", None, Some(placed), None).unwrap();
    // Down on the ground first, then sent east and north.
    for _ in 0..120 {
        game.tick(1.0 / 60.0, None);
    }
    let start = game.engine.world.get::<Transform>(sheep).unwrap().position;
    game.engine.world.get_mut::<BlockMob>(sheep).unwrap().move_to(start + Vec3::new(4.0, 0.0, 4.0));
    let mut left_line = false;
    for _ in 0..240 {
        game.tick(1.0 / 60.0, None);
        let at = game.engine.world.get::<Transform>(sheep).unwrap().position;
        // Going by the corner, it first walks north along its own X.
        left_line |= (at.z - start.z) > 1.0 && (at.x - start.x).abs() < 0.5;
    }
    let asked = ways.0.load(std::sync::atomic::Ordering::Relaxed);
    let at = game.engine.world.get::<Transform>(sheep).unwrap().position;
    assert!(asked > 0, "the navigation set was asked");
    assert!(left_line, "it went by the corner the ways gave, now at {at} from {start}");
    assert!(Vec3::new(at.x - start.x - 4.0, 0.0, at.z - start.z - 4.0).length() < 1.0, "it got there: {at}");
}
