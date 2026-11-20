//! Crowds: Nav Agents and their tiers by how near they are to the game's
//! camera, a Crowd Member's own tier and importance, agents far off
//! planning and moving less often yet getting where they are sent, in less
//! of the step, and a walker kept as a record far off, walking on, made
//! again near and kept in a save (headless: no GPU, no scripts).

use std::time::{Duration, Instant};

use voxear_core::math::{Quat, Vec3};
use voxear_ecs::Entity;
use voxear_engine::{
    Engine, Game, Project,
    crowds::{CrowdMember, CrowdSettings, CrowdTier, CrowdTierChoice, CrowdTiers},
    nav_agent::{NavAgent, NavAgentState, NavMoves},
    physics::CharacterController,
};
use voxear_scene::{Name, Transform};
use voxear_terrain::blocks::{BlockWorld, file::BlockFile};

const FRAME: f32 = 1.0 / 60.0;

/// A field of blocks `size` across and six deep, its corner at the world's.
fn game(size: u32) -> (tempfile::TempDir, Game, Entity) {
    let dir = tempfile::tempdir().unwrap();
    let mut file = BlockFile::default();
    file.ground.size = [size, size];
    file.ground.height = 6;
    let scene =
        "format = 1\n\n[[obj]]\nid = 1\nname = \"Blocks\"\n[obj.block_world]\ndata = \"Blocks/Field.blockworld\"\n";
    let files = [
        ("voxear.toml", "[project]\nname = \"Agents\"\nstart_level = \"Levels/Main.level\"\n".to_string()),
        ("Levels/Main.level", "format = 1\n[[scene]]\nscene = \"Scenes/Field.scene\"\n".to_string()),
        ("Scenes/Field.scene", scene.to_string()),
        ("Blocks/Field.blockworld", file.to_text()),
        ("Blocks/Door.block", "color = [0.5, 0.35, 0.2]\nshape = \"door\"\n".to_string()),
        ("Blocks/Vine.block", "color = [0.2, 0.5, 0.2]\nshape = \"vine\"\n".to_string()),
    ];
    for (path, text) in files {
        let path = dir.path().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    let mut game = Game::new(Project::open(dir.path()).unwrap(), Engine::new()).unwrap();
    game.start(None).unwrap();
    let field = game.engine.world.query_ref::<(Entity, &Name)>().into_iter().find(|(_, n)| n.0 == "Blocks").unwrap().0;
    game.tick(FRAME, None);
    (dir, game, field)
}

/// An agent standing at `(x, z)` on the field's top, moving as `moves` says.
fn agent(game: &mut Game, x: f32, z: f32, moves: NavMoves) -> Entity {
    let at = Transform::from_position(Vec3::new(x, 6.0, z));
    let agent = NavAgent { moves, speed: 3.0, ..NavAgent::default() };
    match moves {
        NavMoves::CharacterController => game.engine.world.spawn((at, agent, CharacterController::default())),
        _ => game.engine.world.spawn((at, agent)),
    }
}

fn at(game: &Game, entity: Entity) -> Vec3 {
    game.engine.world.get::<Transform>(entity).unwrap().position
}

fn arrived(game: &Game, entity: Entity) -> bool {
    let agent = game.engine.world.get::<NavAgent>(entity).unwrap();
    game.engine.world.get::<NavAgentState>(entity).is_some_and(|state| state.is_arrived(agent))
}

/// The game's camera, at `at`.
fn camera(game: &mut Game, at: Vec3) -> Entity {
    game.engine.world.spawn((voxear_render::Camera::default(), Transform::new(at, Quat::IDENTITY, Vec3::ONE)))
}

fn tier(game: &Game, entity: Entity) -> CrowdTier {
    game.engine.world.resource::<CrowdTiers>().map(|tiers| tiers.of(entity)).unwrap_or_default()
}

#[test]
fn agents_are_near_middle_or_far_by_their_distance_unless_a_crowd_member_says() {
    let (_dir, mut game, _) = game(32);
    camera(&mut game, Vec3::new(4.5, 8.0, 4.5));
    let near = agent(&mut game, 10.5, 4.5, NavMoves::AlongTheNav);
    let middle = agent(&mut game, 4.5, 104.5, NavMoves::AlongTheNav);
    let far = agent(&mut game, 304.5, 4.5, NavMoves::AlongTheNav);
    let kept_near = agent(&mut game, 304.5, 8.5, NavMoves::AlongTheNav);
    game.engine.world.insert(kept_near, CrowdMember { tier: CrowdTierChoice::Near, ..CrowdMember::default() });
    let important = agent(&mut game, 4.5, 104.5, NavMoves::AlongTheNav);
    game.engine.world.insert(important, CrowdMember { importance: 4.0, ..CrowdMember::default() });
    game.tick(FRAME, None);
    assert_eq!(tier(&game, near), CrowdTier::Near);
    assert_eq!(tier(&game, middle), CrowdTier::Middle);
    assert_eq!(tier(&game, far), CrowdTier::Far);
    assert_eq!(tier(&game, kept_near), CrowdTier::Near, "its own tier");
    assert_eq!(tier(&game, important), CrowdTier::Near, "a hundred metres off, at four times the importance");
    let counts = game.engine.world.resource::<CrowdTiers>().unwrap().counts;
    assert_eq!(counts, [3, 1, 1]);
    // Tiers switched off, or with no camera at all, every one is near.
    CrowdSettings { enabled: false, ..CrowdSettings::default() }.put(&mut game.engine.world);
    game.tick(FRAME, None);
    assert_eq!(game.engine.world.resource::<CrowdTiers>().unwrap().counts, [5, 0, 0]);
}

/// Sends a crowd of twenty across the field, as the camera at `camera_at` sees
/// it, and gives back how many frames until every one is there.
fn crowd_crossing(moves: NavMoves, camera_at: Vec3) -> usize {
    let (_dir, mut game, _) = game(32);
    camera(&mut game, camera_at);
    let crowd: Vec<Entity> = (0..20).map(|n| agent(&mut game, 2.5 + n as f32 * 1.4, 4.5, moves)).collect();
    for _ in 0..10 {
        game.tick(FRAME, None);
    }
    for (n, walker) in crowd.iter().enumerate() {
        game.engine.world.get_mut::<NavAgent>(*walker).unwrap().go_to(Vec3::new(2.5 + n as f32 * 1.4, 6.0, 26.5));
    }
    let mut frames = 0;
    while frames < 1800 && !crowd.iter().all(|walker| arrived(&game, *walker)) {
        game.tick(FRAME, None);
        frames += 1;
    }
    for walker in &crowd {
        let now = at(&game, *walker);
        assert!(arrived(&game, *walker), "{moves:?}: every one arrived, this one at {now} after {frames} frames");
        assert!(now.z > 25.5, "{moves:?}: across the field: {now}");
        assert!((now.y - 6.0).abs() < 0.2, "{moves:?}: on its top: {now}");
    }
    frames
}

#[test]
fn far_agents_plan_and_move_less_often_yet_get_where_they_are_sent_as_soon() {
    for moves in [NavMoves::AlongTheNav, NavMoves::CharacterController] {
        let near = crowd_crossing(moves, Vec3::new(16.0, 10.0, 16.0));
        let far = crowd_crossing(moves, Vec3::new(616.0, 10.0, 16.0));
        // 22 metres at 3 metres a second: about 7 seconds either way.
        let slower = far as f32 / near as f32;
        assert!((0.9..1.15).contains(&slower), "{moves:?}: near {near} frames, far {far}");
    }
}

/// How long a step of two thousand agents crossing a square both ways takes at least, all far from the camera, or
/// all near with `tiers` off.
fn crowd_step(moves: NavMoves, tiers: bool) -> f64 {
    let size = 316u32;
    let (_dir, mut game, _) = game(size);
    camera(&mut game, Vec3::new(3000.0, 10.0, 158.0));
    CrowdSettings { enabled: tiers, ..CrowdSettings::default() }.put(&mut game.engine.world);
    let far = size as f32 - 4.5;
    let mut agents = Vec::new();
    for row in 0..100 {
        for column in 0..10 {
            let (x, z) = (4.5 + column as f32 * 1.5, 4.5 + row as f32 * 3.0);
            agents.push((agent(&mut game, x, z, moves), Vec3::new(far - column as f32 * 1.5, 6.0, z)));
            agents.push((agent(&mut game, far - column as f32 * 1.5, z + 1.5, moves), Vec3::new(x, 6.0, z + 1.5)));
        }
    }
    for _ in 0..5 {
        game.tick(FRAME, None);
    }
    for (walker, to) in &agents {
        game.engine.world.get_mut::<NavAgent>(*walker).unwrap().go_to(*to);
    }
    for _ in 0..20 {
        game.tick(FRAME, None);
    }
    let mut least = Duration::MAX;
    for _ in 0..60 {
        let began = Instant::now();
        voxear_engine::nav_agent::step_agents(&mut game.engine.world, FRAME);
        least = least.min(began.elapsed());
        game.tick(FRAME, None);
    }
    let moving = agents
        .iter()
        .filter(|(walker, _)| {
            game.engine.world.get::<NavAgentState>(*walker).is_some_and(|s| s.velocity.length() > 0.5)
        })
        .count();
    assert!(moving > agents.len() / 2, "{moves:?}: most of them on their way: {moving}");
    least.as_secs_f64() * 1000.0
}

#[test]
fn two_thousand_far_agents_take_less_of_the_step_than_near_ones() {
    for moves in [NavMoves::AlongTheNav, NavMoves::CharacterController] {
        let near = crowd_step(moves, false);
        let far = crowd_step(moves, true);
        println!("2000 agents {moves:?}: a step in {near:.3} ms near, {far:.3} ms far, at least");
        assert!(far < near * 0.8, "{moves:?}: {far:.3} ms far against {near:.3} ms near");
    }
}

const WALKER: &str = r#"
format = 1

[[obj]]
id = 1
name = "Walker"
[obj.nav_agent]
moves = "along_the_nav"
speed = 3.0
[obj.crowd_member]
keep_as_record = true
"#;

#[test]
fn a_walker_far_off_is_kept_as_a_record_walks_on_and_is_made_again_near() {
    let (dir, mut game, _) = game(32);
    std::fs::create_dir_all(dir.path().join("Prefabs")).unwrap();
    std::fs::write(dir.path().join("Prefabs/Walker.prefab"), WALKER).unwrap();
    game.content.rescan().unwrap();
    let eye = camera(&mut game, Vec3::new(16.0, 10.0, 16.0));
    let placed = voxear_scene::Placement { position: Vec3::new(2.5, 6.0, 2.5), rotation: Quat::IDENTITY };
    let walker = game.instantiate("Prefabs/Walker.prefab", None, Some(placed), None).unwrap();
    let prefab = game.content.db().guid_of("Prefabs/Walker.prefab").unwrap();
    // As a script's copy is marked.
    game.engine.world.insert(walker, voxear_engine::prefab::Spawned { prefab, root: walker, index: 0 });
    for _ in 0..10 {
        game.tick(FRAME, None);
    }
    let to = Vec3::new(26.5, 6.0, 26.5);
    game.engine.world.get_mut::<NavAgent>(walker).unwrap().go_to(to);
    for _ in 0..30 {
        game.tick(FRAME, None);
    }
    let set_off = at(&game, walker);
    assert!(set_off.distance(Vec3::new(2.5, 6.0, 2.5)) > 0.5, "it set off: {set_off}");
    // The camera goes far off: the walker is a record, its Obj gone.
    game.engine.world.get_mut::<Transform>(eye).unwrap().position = Vec3::new(2000.0, 10.0, 16.0);
    for _ in 0..3 {
        game.tick(FRAME, None);
    }
    assert!(!game.engine.world.is_alive(walker), "its Obj went");
    assert_eq!(voxear_engine::crowd_records::count(&game.engine.world), 1);
    // A save keeps it, small.
    let saved = voxear_engine::crowd_records::state(&mut game.engine.world, None).unwrap();
    assert!(saved.len() < 64, "{} bytes", saved.len());
    // It walks on there while no one is near: some 34 metres at 3 a second.
    for _ in 0..60 * 15 {
        game.tick(FRAME, None);
    }
    // The camera comes back: it is made again where it got to, there.
    game.engine.world.get_mut::<Transform>(eye).unwrap().position = Vec3::new(16.0, 10.0, 16.0);
    for _ in 0..3 {
        game.tick(FRAME, None);
    }
    assert_eq!(voxear_engine::crowd_records::count(&game.engine.world), 0);
    let again: Vec<Entity> = game
        .engine
        .world
        .query_ref::<(Entity, &CrowdMember)>()
        .into_iter()
        .filter(|(_, member)| member.keep_as_record)
        .map(|(entity, _)| entity)
        .collect();
    assert_eq!(again.len(), 1, "made again once");
    let now = at(&game, again[0]);
    assert!(Vec3::new(now.x - to.x, 0.0, now.z - to.z).length() < 0.5, "where it was sent: {now}");
    // Put back from the save, the record is where it was then.
    voxear_engine::crowd_records::state(&mut game.engine.world, Some(&saved)).unwrap();
    assert_eq!(voxear_engine::crowd_records::count(&game.engine.world), 1);
    // Drawn while the camera is within 600 m of it, and not beyond.
    let ghosts =
        |game: &Game| game.engine.world.query_ref::<&voxear_engine::crowd_records::RecordGhost>().into_iter().count();
    // 400 m off, the walker made again goes as a record too: both drawn.
    game.engine.world.get_mut::<Transform>(eye).unwrap().position = Vec3::new(400.0, 10.0, 16.0);
    game.tick(FRAME, None);
    assert_eq!(voxear_engine::crowd_records::count(&game.engine.world), 2, "both records 400 m off");
    assert_eq!(ghosts(&game), 2, "both drawn");
    game.engine.world.get_mut::<Transform>(eye).unwrap().position = Vec3::new(2000.0, 10.0, 16.0);
    game.tick(FRAME, None);
    assert_eq!(ghosts(&game), 0, "too far to draw");
}

/// A project with a camera at (0, 1, 0) looking along +Z through a narrow lens, a sun, and a prefab of the
/// infinite-world template's player playing a clip, `Wave`, its right arm held straight up.
fn drawing_project() -> tempfile::TempDir {
    use voxear_render::{BoxClip, BoxKey, BoxModel, BoxTrack};
    let dir = tempfile::tempdir().unwrap();
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../templates/infinite-world/Characters/Player.boxmodel");
    let mut model = BoxModel::parse(&std::fs::read_to_string(path).unwrap()).unwrap();
    let up = Vec3::new(180.0, 0.0, 0.0);
    let held = BoxTrack { bone: "RightArm".into(), keys: vec![BoxKey::turned(0.0, up), BoxKey::turned(1.0, up)] };
    model.clips.push(BoxClip { tracks: vec![held], ..BoxClip::new("Wave") });
    let scene = "format = 1\n\n[[obj]]\nid = 1\nname = \"Eye\"\nposition = [0.0, 1.0, 0.0]\n[obj.camera]\nfov = 4.0\n\n\
                 [[obj]]\nid = 2\nname = \"Sun\"\nrotation = [50.0, 160.0, 0.0]\n[obj.light]\ntype = \"directional\"\n";
    let prefab = "format = 1\n\n[[obj]]\nid = 1\nname = \"Waver\"\n[obj.model]\nmodel = \"Characters/Player.boxmodel\"\n\
                  [obj.animator]\nclip = \"Wave\"\n[obj.crowd_member]\nkeep_as_record = true\n";
    let files = [
        ("voxear.toml", "[project]\nname = \"Drawn\"\nstart_level = \"Levels/Main.level\"\n".to_string()),
        ("Levels/Main.level", "format = 1\n[[scene]]\nscene = \"Scenes/Main.scene\"\n".to_string()),
        ("Scenes/Main.scene", scene.to_string()),
        ("Characters/Player.boxmodel", model.to_text()),
        ("Characters/Waver.prefab", prefab.to_string()),
    ];
    for (path, text) in files {
        let path = dir.path().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    dir
}

#[test]
fn a_record_far_off_is_drawn_as_its_prefabs_model_playing_its_clip() {
    use voxear_engine::crowd_records::{CrowdRecord, CrowdRecords, RecordGhost, state};
    use voxear_render::{Renderer, RendererSettings};
    let Ok(mut renderer) = Renderer::new_offscreen(640, 360, &RendererSettings::default()) else {
        eprintln!("skipping: no usable GPU");
        return;
    };
    let dir = drawing_project();
    let mut game = Game::new(Project::open(dir.path()).unwrap(), Engine::new()).unwrap();
    game.start(Some(&mut renderer)).unwrap();
    // One record, standing 400 m straight ahead: beyond where records are made Objs, within where they are drawn.
    let prefab = game.content.db().guid_of("Characters/Waver.prefab").unwrap();
    let mut records = CrowdRecords::default();
    let at = voxear_core::math::DVec3::new(0.0, 0.0, 400.0);
    let yaw = std::f32::consts::PI;
    records.records.push(CrowdRecord {
        id: 1,
        prefab,
        position: at,
        yaw,
        destination: None,
        way: Vec::new(),
        speed: 0.0,
    });
    state(&mut game.engine.world, Some(&records.to_bytes())).unwrap();
    for _ in 0..30 {
        game.tick(FRAME, Some(&mut renderer));
        renderer.render(&game.engine.world, 1.0).unwrap();
    }
    let ghosts = game.engine.world.query_ref::<&RecordGhost>().into_iter().count();
    assert_eq!(ghosts, 2, "the ghost and its model");
    let played = game.engine.world.query_ref::<&voxear_render::PlayedClip>().into_iter().count();
    assert_eq!(played, 1, "its model plays Wave on the GPU");
    assert_eq!(renderer.stats().posed.baked, 1, "{:?}", renderer.stats().posed);
    // 400 m off, a figure about 24 pixels tall just under the middle of the picture, darker than the haze beside it.
    let far = picture(&mut renderer, "record ghost far.png");
    let haze = count(&far, (200, 179, 201, 180), |_| true);
    assert_eq!(haze, 1);
    let at = ((180 * far.0 + 200) * 4) as usize;
    let beside = [far.1[at], far.1[at + 1], far.1[at + 2]].map(i32::from);
    let figure = count(&far, (300, 160, 340, 200), |rgb| (0..3).all(|c| rgb[c] + 6 < beside[c]));
    assert!(figure > 60, "a figure shows: {figure} pixels");
    // Up close, still a ghost (no record made an Obj), facing the camera with its right arm raised by its clip.
    voxear_engine::crowds::CrowdSettings { records_beyond: 0.0, ..Default::default() }.put(&mut game.engine.world);
    let eye = game.engine.world.query_ref::<(Entity, &voxear_render::Camera)>().into_iter().next().unwrap().0;
    game.engine.world.get_mut::<Transform>(eye).unwrap().position = Vec3::new(0.0, 1.0, 394.0);
    game.engine.world.insert(eye, voxear_render::Camera { fov_y_degrees: 60.0, ..Default::default() });
    for _ in 0..10 {
        game.tick(FRAME, Some(&mut renderer));
        renderer.render(&game.engine.world, 1.0).unwrap();
    }
    assert_eq!(game.engine.world.query_ref::<&RecordGhost>().into_iter().count(), 2, "still a ghost");
    let near = picture(&mut renderer, "record ghost near.png");
    let shirt = count(&near, (300, 160, 345, 195), |[r, g, b]| r > 2 * g + 20 && r > 2 * b + 20);
    assert!(shirt > 200, "its red shirt: {shirt} pixels");
    // Where its right arm hangs at rest is beside its body; held up, beside its head.
    let raised = count(&near, (296, 136, 306, 156), |[r, _, b]| b + 15 < r);
    assert!(raised > 40, "its arm held up beside its head: {raised} pixels");
}

/// The picture drawn last, kept as `name` where `VOXEAR_TEST_OUTPUT` says.
fn picture(renderer: &mut voxear_render::Renderer, name: &str) -> (u32, Vec<u8>) {
    let (width, height, pixels) = renderer.read_pixels().unwrap();
    if let Ok(out) = std::env::var("VOXEAR_TEST_OUTPUT") {
        let file = std::fs::File::create(std::path::Path::new(&out).join(name)).unwrap();
        let mut encoder = png::Encoder::new(file, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.write_header().unwrap().write_image_data(&pixels).unwrap();
    }
    (width, pixels)
}

/// How many pixels in a rectangle, left, top, right and bottom, are as `wanted` says of their colour.
fn count(
    (width, pixels): &(u32, Vec<u8>),
    (left, top, right, bottom): (u32, u32, u32, u32),
    wanted: impl Fn([i32; 3]) -> bool,
) -> usize {
    let mut found = 0;
    for y in top..bottom {
        for x in left..right {
            let at = ((y * width + x) * 4) as usize;
            let rgb = [pixels[at], pixels[at + 1], pixels[at + 2]].map(i32::from);
            found += usize::from(wanted(rgb));
        }
    }
    found
}
