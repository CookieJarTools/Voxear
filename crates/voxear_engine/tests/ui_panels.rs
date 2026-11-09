//! UI Panels in the world, without a GPU: a panel shows its document, a ray
//! from the camera works its button, and something in front stops it.

use std::collections::HashMap;

use voxear_core::math::{Quat, Vec3};
use voxear_ecs::{Entity, World};
use voxear_engine::{Content, game_ui::GameUi, scene_file::SceneFile};
use voxear_render::UiPanel;
use voxear_scene::Transform;
use winit::event::{DeviceId, ElementState, MouseButton, WindowEvent};

const TERMINAL: &str = "0000000000000000000000000000ab01";

/// A project with a terminal's document: a button filling its right half.
fn project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("UI")).unwrap();
    let markup = r#"<vui><Panel name="screen" style="flex-direction: row; width: 100%; height: 100%"><Panel style="flex-grow: 1" /><Button name="go" style="flex-grow: 1; height: 100%">Go</Button></Panel></vui>"#;
    std::fs::write(dir.path().join("UI/Terminal.vui"), markup).unwrap();
    std::fs::write(dir.path().join("UI/Terminal.vui.meta"), format!("guid = \"{TERMINAL}\"\n")).unwrap();
    dir
}

/// A 2 by 1 metre panel showing the terminal, five metres ahead of the
/// origin, read looking along +Z.
fn terminal(world: &mut World) -> Entity {
    let panel = UiPanel { document: Some(TERMINAL.parse().unwrap()), size: [2.0, 1.0], ..UiPanel::default() };
    world.spawn((Transform::new(Vec3::new(0.0, 0.0, 5.0), Quat::IDENTITY, Vec3::ONE), panel))
}

fn click(ui: &mut GameUi, state: ElementState) -> bool {
    ui.panel_input(&WindowEvent::MouseInput { device_id: DeviceId::dummy(), state, button: MouseButton::Left })
}

#[test]
fn a_ray_clicks_a_panels_button_unless_something_is_in_front() {
    let dir = project();
    let mut content = Content::open(dir.path()).unwrap();
    let mut world = World::new();
    let panel = terminal(&mut world);
    let mut ui = GameUi::default();
    ui.sync_panels(&mut world, &mut content, &HashMap::new());
    let document = ui.panel_document(panel).expect("it shows its document");
    assert_eq!(world.get::<voxear_engine::ui_panels::PanelDocument>(panel).map(|d| d.0), Some(document));
    let go = ui.find(document, "go").expect("its button, found as on the screen");
    let [(_, size, _)] = &ui.lay_out_panels(1.0 / 60.0)[..] else { panic!("one panel") };
    assert_eq!(*size, [1000, 500], "laid out at 500 pixels a metre");

    // At the button, on the panel's right half: pressed and let go, a click the game does not see.
    let at_button = Some((Vec3::ZERO, Vec3::new(0.5, 0.0, 5.0).normalize()));
    ui.point_panels(&world, at_button, |_, _| None);
    assert_eq!(ui.hovered_panel(), Some(panel));
    assert!(click(&mut ui, ElementState::Pressed), "the panel takes the press");
    assert!(click(&mut ui, ElementState::Released), "and its release");
    assert!(ui.clicked(go), "the button heard the click");
    ui.lay_out_panels(1.0 / 60.0);
    assert!(!ui.clicked(go), "once");

    // Something solid a metre ahead stops the ray before it reaches the panel.
    ui.point_panels(&world, at_button, |_, _| Some(1.0));
    assert_eq!(ui.hovered_panel(), None);
    assert!(!click(&mut ui, ElementState::Pressed), "the game has the press");
    assert!(!click(&mut ui, ElementState::Released));

    // Not clickable: the pointer passes through.
    world.get_mut::<UiPanel>(panel).unwrap().clickable = false;
    ui.point_panels(&world, at_button, |_, _| None);
    assert_eq!(ui.hovered_panel(), None);

    // Gone: its UI goes with it.
    world.despawn(panel);
    ui.sync_panels(&mut world, &mut content, &HashMap::new());
    assert!(ui.panel_document(panel).is_none());
    assert!(ui.find(document, "go").is_none(), "its numbers name nothing now");
}

#[test]
fn a_panel_is_drawn_larger_as_it_shows_larger() {
    let dir = project();
    let mut content = Content::open(dir.path()).unwrap();
    let mut world = World::new();
    let panel = terminal(&mut world);
    let mut ui = GameUi::default();
    let shown = |size: [f32; 2]| HashMap::from([(panel, size)]);
    ui.sync_panels(&mut world, &mut content, &shown([1900.0, 950.0]));
    assert_eq!(ui.lay_out_panels(0.0)[0].1, [2000, 1000], "nearly twice its layout: twice");
    ui.sync_panels(&mut world, &mut content, &shown([300.0, 150.0]));
    assert_eq!(ui.lay_out_panels(0.0)[0].1, [500, 250], "far off: smaller, a step above it");
}

#[test]
fn a_scene_keeps_a_panels_settings() {
    let dir = project();
    let content = Content::open(dir.path()).unwrap();
    let text = format!(
        "format = 1\n[[obj]]\nid = 1\nname = \"Terminal\"\n[obj.ui_panel]\ndocument = \"UI/Terminal.vui\"\nsize = [2.0, 1.0]\nface_camera = true\n"
    );
    let scene: SceneFile = toml::from_str(&text).unwrap();
    let data = scene.objs[0].ui_panel.clone().expect("read from the scene");
    let panel = voxear_engine::ui_panels::ui_panel(&content, &data).unwrap();
    assert_eq!(panel.document, Some(TERMINAL.parse().unwrap()), "found by its path");
    assert!(panel.face_camera && panel.clickable);
    let written = voxear_engine::ui_panels::ui_panel_data(&panel);
    assert_eq!(written.document.as_deref(), Some(TERMINAL), "written back by GUID");
    assert_eq!(toml::to_string(&written).unwrap(), format!("document = \"{TERMINAL}\"\nsize = [2.0, 1.0]\nface_camera = true\n"));
}

#[test]
fn a_two_sided_panel_is_clicked_from_behind_the_right_way_round() {
    let dir = project();
    let mut content = Content::open(dir.path()).unwrap();
    let mut world = World::new();
    let panel = terminal(&mut world);
    let mut ui = GameUi::default();
    ui.sync_panels(&mut world, &mut content, &HashMap::new());
    ui.lay_out_panels(1.0 / 60.0);
    let go = ui.find(ui.panel_document(panel).unwrap(), "go").unwrap();

    // From behind, looking back along -Z, the button on the page's right half is on the viewer's right: -X.
    let behind = Vec3::new(0.0, 0.0, 10.0);
    let at_button = Some((behind, Vec3::new(-0.5, 0.0, -5.0).normalize()));
    ui.point_panels(&world, at_button, |_, _| None);
    assert_eq!(ui.hovered_panel(), Some(panel), "two-sided, as a panel is unless told otherwise");
    assert!(click(&mut ui, ElementState::Pressed) && click(&mut ui, ElementState::Released));
    assert!(ui.clicked(go), "the button, where it reads from behind");
    ui.lay_out_panels(1.0 / 60.0);

    // The page's left half, from behind, is not the button.
    ui.point_panels(&world, Some((behind, Vec3::new(0.5, 0.0, -5.0).normalize())), |_, _| None);
    click(&mut ui, ElementState::Pressed);
    click(&mut ui, ElementState::Released);
    assert!(!ui.clicked(go));
    ui.lay_out_panels(1.0 / 60.0);

    // One-sided, there is nothing to point at from behind.
    world.get_mut::<UiPanel>(panel).unwrap().two_sided = false;
    ui.point_panels(&world, at_button, |_, _| None);
    assert_eq!(ui.hovered_panel(), None);
}

#[test]
fn a_panels_document_loads_when_first_asked_for() {
    let dir = project();
    let mut content = Content::open(dir.path()).unwrap();
    let mut world = World::new();
    let panel = terminal(&mut world);
    let mut ui = GameUi::default();

    // Before it is ever drawn, as a script's Start asks the frame its Obj is made.
    let document = ui.load_panel(&mut world, &mut content, panel).expect("loaded when asked for");
    assert_eq!(world.get::<voxear_engine::ui_panels::PanelDocument>(panel).map(|d| d.0), Some(document));
    assert!(ui.find(document, "go").is_some());
    ui.sync_panels(&mut world, &mut content, &HashMap::new());
    assert_eq!(ui.panel_document(panel), Some(document), "drawing keeps the one loaded");
    assert_eq!(ui.load_panel(&mut world, &mut content, panel), Some(document), "and so does asking again");
}

#[test]
fn a_panel_switched_off_hides_and_keeps_its_elements() {
    let dir = project();
    let mut content = Content::open(dir.path()).unwrap();
    let mut world = World::new();
    let panel = terminal(&mut world);
    let mut ui = GameUi::default();
    ui.sync_panels(&mut world, &mut content, &HashMap::new());
    ui.lay_out_panels(1.0 / 60.0);
    let document = ui.panel_document(panel).unwrap();
    let go = ui.find(document, "go").unwrap();
    let at_button = Some((Vec3::ZERO, Vec3::new(0.5, 0.0, 5.0).normalize()));

    // Off: nothing to point at, and its UI kept for when it is back.
    voxear_scene::set_active(&mut world, panel, false);
    ui.sync_panels(&mut world, &mut content, &HashMap::new());
    ui.point_panels(&world, at_button, |_, _| None);
    assert_eq!(ui.hovered_panel(), None);
    assert!(voxear_render::ui_panel_rects(&world, Vec3::ZERO).is_empty(), "not drawn");
    assert_eq!(ui.panel_document(panel), Some(document));

    // Back on: the same button, clicked.
    voxear_scene::set_active(&mut world, panel, true);
    ui.sync_panels(&mut world, &mut content, &HashMap::new());
    ui.lay_out_panels(1.0 / 60.0);
    ui.point_panels(&world, at_button, |_, _| None);
    assert!(click(&mut ui, ElementState::Pressed) && click(&mut ui, ElementState::Released));
    assert_eq!(ui.find(document, "go"), Some(go));
    assert!(ui.clicked(go));
}

#[test]
fn a_saved_document_keeps_the_numbers_of_its_elements() {
    let dir = project();
    let mut content = Content::open(dir.path()).unwrap();
    let mut world = World::new();
    let panel = terminal(&mut world);
    let mut ui = GameUi::default();
    ui.sync_panels(&mut world, &mut content, &HashMap::new());
    let document = ui.panel_document(panel).unwrap();
    let go = ui.find(document, "go").unwrap();
    let root = ui.find(document, "").unwrap();
    let made = ui.create(root, voxear_ui::ElementKind::Label, "Made in code").unwrap();
    let file = dir.path().join("UI/Terminal.vui");
    let markup = std::fs::read_to_string(&file).unwrap();

    // Saved with the button's text changed: the same numbers, now the new elements.
    std::fs::write(&file, markup.replace(">Go<", ">Open<")).unwrap();
    ui.reload_panels(&mut content);
    assert_eq!(ui.panel_document(panel), Some(document));
    assert_eq!(ui.find(document, "go"), Some(go));
    assert_eq!(ui.text(go).as_deref(), Some("Open"));
    assert_eq!(ui.text(made).as_deref(), Some("Made in code"), "what a script added comes along");
    ui.lay_out_panels(1.0 / 60.0);
    ui.point_panels(&world, Some((Vec3::ZERO, Vec3::new(0.5, 0.0, 5.0).normalize())), |_, _| None);
    assert!(click(&mut ui, ElementState::Pressed) && click(&mut ui, ElementState::Released));
    assert!(ui.clicked(go), "the new button, clicked by its old number");

    // A file that cannot be read leaves it as it was.
    std::fs::write(&file, "<vui><Panel").unwrap();
    ui.reload_panels(&mut content);
    assert_eq!(ui.text(go).as_deref(), Some("Open"));

    // Without the button now: its number stands for nothing.
    std::fs::write(&file, "<vui><Label name=\"said\">Closed</Label></vui>").unwrap();
    ui.reload_panels(&mut content);
    assert_eq!(ui.find(document, "go"), None);
    assert_eq!(ui.text(go), None);
    assert!(ui.find(document, "said").is_some());
}
