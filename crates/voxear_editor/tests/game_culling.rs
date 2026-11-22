//! Gizmos ▸ Game Camera's Culling, on copies of Hello played beside a Scene
//! view: an Obj the game's camera sees only through a wall is not drawn in
//! the Scene view while it is on, though the Scene view's own camera sees it,
//! and is again with it off; and the part of a world of blocks behind the
//! game's camera goes from the Scene view the same way. Also times the late
//! cull on the GPU each way, the least of sixty frames. Skips itself without
//! a GPU.

use voxear_core::{
    logging::LogCapture,
    math::{Quat, Vec3},
};
use voxear_engine::Project;
use voxear_render::{Renderer, RendererSettings};
use voxear_scene::Transform;

const SIZE: (u32, u32) = (1280, 800);

/// Hello's sky and a sun, for a Day scene of the test's own.
const SKY_AND_SUN: &str = r#"# Voxear scene
format = 1

[sky]
zenith = [0.517, 0.701, 0.931]
horizon = [0.809, 0.865, 0.936]
ground = [0.506, 0.485, 0.461]
ambient = 1.0

[[obj]]
id = 1
name = "Sun"
rotation = [42.0, -35.0, 0.0]
light = { type = "directional", color = [1.0, 0.95, 0.88], intensity = 3.2 }
"#;

/// A wall straight ahead of Hello's Main Camera, at (0, 2.2, -7) looking
/// along +z, filling its view, a green box behind it, and a red one in front
/// of it.
const WALL_AND_BOXES: &str = r#"
[[obj]]
id = 2
name = "Wall"
position = [0.0, 6.0, 0.0]
scale = [40.0, 12.0, 0.5]
[obj.renderer]
mesh = "builtin:cube"
material = { color = [0.5, 0.5, 0.5] }

[[obj]]
id = 3
name = "Hidden"
position = [0.0, 1.0, 5.0]
scale = [1.5, 1.5, 1.5]
[obj.renderer]
mesh = "builtin:cube"
material = { color = [0.0, 0.0, 0.0], emissive = [0.0, 6.0, 0.0] }

[[obj]]
id = 4
name = "Seen"
position = [0.5, 0.75, -3.5]
scale = [0.8, 0.8, 0.8]
[obj.renderer]
mesh = "builtin:cube"
material = { color = [0.0, 0.0, 0.0], emissive = [6.0, 0.0, 0.0] }
"#;

/// A flat world of blocks of grass 256 blocks a side, its top at y = 0 and
/// its middle near the game's camera: its chunks, 32 blocks a side, from
/// 39 m behind the camera wholly behind it.
const FIELD: &str = r#"
[[obj]]
id = 2
name = "Field"
position = [-128.0, -8.0, -128.0]
[obj.block_world]
data = "Parts/Field.blockworld"
"#;

/// A copy of Hello inside the repository whose Day scene is `day`, without
/// its scripts; and without its ground, for a world of blocks to show.
fn hello_with(day: &str, ground: bool) -> tempfile::TempDir {
    fn copy(from: &std::path::Path, to: &std::path::Path) {
        std::fs::create_dir_all(to).unwrap();
        for entry in std::fs::read_dir(from).unwrap().flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') || name == "bin" || name == "obj" {
                continue;
            }
            if entry.path().is_dir() {
                copy(&entry.path(), &to.join(&name));
            } else {
                std::fs::copy(entry.path(), to.join(&name)).unwrap();
            }
        }
    }
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    copy(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../samples/Hello"), dir.path());
    std::fs::write(dir.path().join("Scenes/Day.scene"), format!("{SKY_AND_SUN}{day}")).unwrap();
    if !ground {
        let environment = dir.path().join("Scenes/Environment.scene");
        let scene = std::fs::read_to_string(&environment).unwrap();
        let plane = scene.find("[obj.renderer]\nmesh = \"builtin:plane\"").expect("Hello's ground");
        let end = scene[plane..].find("\n\n").map_or(scene.len(), |end| plane + end);
        std::fs::write(&environment, format!("{}{}", &scene[..plane], &scene[end..])).unwrap();
    }
    let toml = dir.path().join("voxear.toml");
    let text = std::fs::read_to_string(&toml).unwrap();
    std::fs::write(&toml, text.lines().filter(|line| !line.starts_with("scripts")).collect::<Vec<_>>().join("\n"))
        .unwrap();
    dir
}

fn frame(editor: &mut voxear_editor::Editor, renderer: &mut Renderer) {
    editor.update(renderer, SIZE, 1.0, 1.0 / 60.0).unwrap();
}

/// Enough frames for the frames in flight, last frame's visibility and the
/// picture's history to catch up.
fn settle(editor: &mut voxear_editor::Editor, renderer: &mut Renderer) {
    for _ in 0..24 {
        frame(editor, renderer);
    }
}

/// The editor on `dir`, the Scene and Game views side by side, playing,
/// with the Scene view's camera at `place`; or `None` without a GPU.
fn playing(dir: &tempfile::TempDir, place: Transform) -> Option<(Renderer, voxear_editor::Editor)> {
    let mut renderer = match Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()) {
        Ok(renderer) => renderer,
        Err(error) => {
            eprintln!("skipping: no usable GPU ({error:#})");
            return None;
        }
    };
    let project = Project::open(dir.path()).unwrap();
    let mut editor = voxear_editor::Editor::open_without_memory(project, &mut renderer, LogCapture::default()).unwrap();
    let dock = editor.ui.find(editor.ui.root(), "dock").unwrap();
    editor.ui.set_dock_layout_closing_rest(dock, "row(50%, [scene], [game])").unwrap();
    frame(&mut editor, &mut renderer);
    editor.play(&mut renderer);
    let started = std::time::Instant::now();
    while !editor.is_playing() {
        assert_eq!(editor.play_error(), None, "play starts");
        assert!(started.elapsed().as_secs() < 120, "play takes too long to start");
        frame(&mut editor, &mut renderer);
    }
    let (_, camera) = editor.scene_view_cameras()[0].clone();
    *editor.game.engine.world.get_mut::<Transform>(camera).unwrap() = place;
    settle(&mut editor, &mut renderer);
    Some((renderer, editor))
}

/// Sets Game Camera's Culling through the editor's link, as `vox editor
/// call scene_view.culling` does, and lets it take hold.
fn culling(editor: &mut voxear_editor::Editor, renderer: &mut Renderer, on: bool) -> serde_json::Value {
    let answer = editor.call("scene_view.culling", serde_json::json!({ "on": on }));
    frame(editor, renderer);
    let answer = answer.try_recv().expect("answered within a frame").expect("the call succeeds");
    settle(editor, renderer);
    answer
}

/// How many of the pixels round where `point` falls in the Scene view are
/// `like` it, in the rendered picture.
fn share_round(editor: &voxear_editor::Editor, renderer: &mut Renderer, point: Vec3, like: fn([u8; 3]) -> bool) -> f32 {
    let camera = editor.scene_camera().expect("the Scene view shows");
    let [x, y] = camera.project(point).expect("in front of the Scene view's camera");
    let (width, _, pixels) = renderer.read_pixels().unwrap();
    let (x, y) = (x.round() as i64, y.round() as i64);
    let mut counted = 0;
    let mut alike = 0;
    for dy in -4..=4 {
        for dx in -4..=4 {
            let at = (((y + dy) * width as i64 + x + dx) * 4) as usize;
            counted += 1;
            alike += usize::from(like([pixels[at], pixels[at + 1], pixels[at + 2]]));
        }
    }
    alike as f32 / counted as f32
}

/// With `GAME_CULLING_SHOTS` naming a folder, the editor's picture as it is
/// now, saved there as `name`, to look at.
fn shot(renderer: &mut Renderer, name: &str) {
    let Some(dir) = std::env::var_os("GAME_CULLING_SHOTS") else { return };
    let (width, height, pixels) = renderer.read_pixels().unwrap();
    let path = std::path::Path::new(&dir).join(name);
    image::save_buffer(path, &pixels, width, height, image::ExtendedColorType::Rgba8).unwrap();
}

fn green([r, g, b]: [u8; 3]) -> bool {
    g > r.saturating_add(40) && g > b.saturating_add(40)
}

fn red([r, g, b]: [u8; 3]) -> bool {
    r > g.saturating_add(40) && r > b.saturating_add(40)
}

/// The grass's green, lit: greener than it is red or blue.
fn grass([r, g, b]: [u8; 3]) -> bool {
    f32::from(g) > f32::from(r) * 1.15 && f32::from(g) > f32::from(b) * 1.4
}

/// The least time the late cull took on the GPU over sixty frames, of every
/// view, in milliseconds.
fn least_late_cull(editor: &mut voxear_editor::Editor, renderer: &mut Renderer) -> f32 {
    let mut least = f32::MAX;
    for _ in 0..60 {
        frame(editor, renderer);
        let took: f32 =
            renderer.timings().iter().filter(|(name, _)| name.ends_with("Cull (late)")).map(|(_, ms)| ms).sum();
        least = least.min(took);
    }
    least
}

#[test]
fn an_obj_the_game_camera_sees_only_through_a_wall_is_not_drawn_in_the_scene_view() {
    let dir = hello_with(WALL_AND_BOXES, true);
    // High over the wall, looking straight down on both boxes, with nothing between.
    let place = Transform::new(Vec3::new(0.0, 40.0, 2.0), Quat::from_rotation_x(90f32.to_radians()), Vec3::ONE);
    let Some((mut renderer, mut editor)) = playing(&dir, place) else { return };
    let (hidden, seen) = (Vec3::new(0.0, 1.0, 5.0), Vec3::new(0.5, 0.75, -3.5));

    let answer = culling(&mut editor, &mut renderer, true);
    assert_eq!(answer["on"], true, "{answer}");
    assert!(answer["camera"].is_object(), "it culls from the game's camera: {answer}");
    let occluded = renderer.stats().occlusion_culled;
    let behind = share_round(&editor, &mut renderer, hidden, green);
    let before = share_round(&editor, &mut renderer, seen, red);
    shot(&mut renderer, "wall-culled-from-game.png");
    let on_ms = least_late_cull(&mut editor, &mut renderer);

    let answer = culling(&mut editor, &mut renderer, false);
    assert_eq!(answer["on"], false, "{answer}");
    assert!(answer["camera"].is_null(), "it culls from its own camera: {answer}");
    let own_occluded = renderer.stats().occlusion_culled;
    let behind_own = share_round(&editor, &mut renderer, hidden, green);
    let before_own = share_round(&editor, &mut renderer, seen, red);
    shot(&mut renderer, "wall-culled-from-own.png");
    let off_ms = least_late_cull(&mut editor, &mut renderer);
    eprintln!(
        "late cull, least of 60 frames, every view: {on_ms:.4} ms culled from the game's camera, {off_ms:.4} ms own"
    );

    assert!(behind < 0.1, "the box behind the wall is not drawn while culled from the game's camera: {behind}");
    assert!(behind_own > 0.6, "and is, culled from the Scene view's own: {behind_own}");
    assert!(before > 0.6 && before_own > 0.6, "the box in front of it is drawn either way: {before}, {before_own}");
    assert!(
        occluded > own_occluded,
        "the Scene view's cull hid more from the game's camera: {occluded}, {own_occluded}"
    );
}

#[test]
fn the_blocks_behind_the_game_camera_are_not_drawn_in_the_scene_view() {
    let dir = hello_with(FIELD, false);
    {
        let parts = dir.path().join("Parts");
        std::fs::create_dir_all(&parts).unwrap();
        let mut file = voxear_terrain::blocks::file::BlockFile::default();
        file.ground.size = [256, 256];
        std::fs::write(parts.join("Field.blockworld"), file.to_text()).unwrap();
    }
    // High over the field, looking straight down.
    let place = Transform::new(Vec3::new(0.0, 90.0, -40.0), Quat::from_rotation_x(90f32.to_radians()), Vec3::ONE);
    let Some((mut renderer, mut editor)) = playing(&dir, place) else { return };
    let (behind, ahead) = (Vec3::new(0.0, 0.0, -75.0), Vec3::new(0.0, 0.0, -2.0));

    culling(&mut editor, &mut renderer, true);
    let (behind_on, ahead_on) =
        (share_round(&editor, &mut renderer, behind, grass), share_round(&editor, &mut renderer, ahead, grass));
    shot(&mut renderer, "blocks-culled-from-game.png");
    culling(&mut editor, &mut renderer, false);
    let (behind_off, ahead_off) =
        (share_round(&editor, &mut renderer, behind, grass), share_round(&editor, &mut renderer, ahead, grass));
    shot(&mut renderer, "blocks-culled-from-own.png");

    assert!(behind_on < 0.1, "the field behind the game's camera is not drawn: {behind_on}");
    assert!(behind_off > 0.6, "and is, culled from the Scene view's own camera: {behind_off}");
    assert!(ahead_on > 0.6 && ahead_off > 0.6, "the field ahead of it is drawn either way: {ahead_on}, {ahead_off}");
}
