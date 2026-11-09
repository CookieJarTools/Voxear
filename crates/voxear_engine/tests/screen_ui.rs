//! Screen UIs and the screen's documents, without a GPU: a component's
//! document shown while its Obj is on, over others by its order, and closed
//! with its Obj; documents loaded again as their files change, keeping their
//! numbers and their places; and one style property set, keeping the rest.

use voxear_core::math::{Quat, Vec3};
use voxear_ecs::{Entity, World};
use voxear_engine::{
    Content,
    game_ui::GameUi,
    screen_ui::{ScreenDocument, ScreenUi},
};
use voxear_scene::Transform;
use voxear_scripting::UiAsk;

const HUD: &str = "0000000000000000000000000000ab02";
const MENU: &str = "0000000000000000000000000000ab03";

const HUD_MARKUP: &str =
    r#"<vui><Panel name="bar" style="width: 100px; height: 10px" /><Label name="coins">0</Label></vui>"#;
const MENU_MARKUP: &str = r#"<vui><Label name="title">Paused</Label></vui>"#;

/// A project with a HUD and a menu.
fn project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("UI")).unwrap();
    for (name, guid, markup) in [("Hud", HUD, HUD_MARKUP), ("Menu", MENU, MENU_MARKUP)] {
        std::fs::write(dir.path().join(format!("UI/{name}.vui")), markup).unwrap();
        std::fs::write(dir.path().join(format!("UI/{name}.vui.meta")), format!("guid = \"{guid}\"\n")).unwrap();
    }
    dir
}

fn screen(world: &mut World, guid: &str, order: i32) -> Entity {
    let screen = ScreenUi { document: Some(guid.parse().unwrap()), order };
    world.spawn((Transform::new(Vec3::ZERO, Quat::IDENTITY, Vec3::ONE), screen))
}

/// Where the document holding the element named `name` is among the
/// screen's, counting from the bottom.
fn place(ui: &mut GameUi, name: &str) -> usize {
    let ui = ui.ui();
    let root = ui.root();
    let node = ui.find(root, name).expect("on the screen");
    ui.children(root).into_iter().position(|document| ui.is_within(node, document)).unwrap()
}

#[test]
fn a_screen_ui_shows_its_document_while_its_obj_is_on() {
    let dir = project();
    let mut content = Content::open(dir.path()).unwrap();
    let mut world = World::new();
    let hud = screen(&mut world, HUD, 1);
    let menu = screen(&mut world, MENU, 0);
    let mut ui = GameUi::default();
    ui.sync_screens(&mut world, &mut content);
    let document = ui.screen_document(hud).expect("the HUD shows");
    assert_eq!(world.get::<ScreenDocument>(hud).map(|d| d.0), Some(document));
    assert!(ui.find(document, "coins").is_some());
    assert!(place(&mut ui, "coins") > place(&mut ui, "title"), "the HUD, order 1, over the menu");

    // Off: hidden, its elements kept; back on: shown.
    let whole = ui.find(document, "").unwrap();
    voxear_scene::set_active(&mut world, hud, false);
    ui.sync_screens(&mut world, &mut content);
    assert!(!ui.visible(whole));
    assert_eq!(ui.screen_document(hud), Some(document));
    voxear_scene::set_active(&mut world, hud, true);
    ui.sync_screens(&mut world, &mut content);
    assert!(ui.visible(whole));

    // A higher order puts the menu over it.
    world.get_mut::<ScreenUi>(menu).unwrap().order = 2;
    ui.sync_screens(&mut world, &mut content);
    assert!(place(&mut ui, "title") > place(&mut ui, "coins"));

    // Gone: closed.
    world.despawn(hud);
    ui.sync_screens(&mut world, &mut content);
    assert_eq!(ui.screen_document(hud), None);
    assert!(ui.find(document, "coins").is_none());
}

#[test]
fn a_saved_document_keeps_its_numbers_and_its_place() {
    let dir = project();
    let mut content = Content::open(dir.path()).unwrap();
    let mut world = World::new();
    let mut ui = GameUi::default();
    // One a script loaded, and the HUD over it.
    let menu = ui.load(&mut content, "UI/Menu.vui").unwrap();
    let hud = screen(&mut world, HUD, 0);
    ui.sync_screens(&mut world, &mut content);
    let document = ui.screen_document(hud).unwrap();
    let coins = ui.find(document, "coins").unwrap();
    let title = ui.find(menu, "title").unwrap();

    std::fs::write(dir.path().join("UI/Hud.vui"), HUD_MARKUP.replace(">0<", ">10<")).unwrap();
    std::fs::write(dir.path().join("UI/Menu.vui"), MENU_MARKUP.replace("Paused", "Stopped")).unwrap();
    ui.reload_documents(&mut content);
    assert_eq!(ui.find(document, "coins"), Some(coins));
    assert_eq!(ui.text(coins).as_deref(), Some("10"));
    assert_eq!(ui.text(title).as_deref(), Some("Stopped"), "the one the script loaded too");
    assert!(place(&mut ui, "coins") > place(&mut ui, "title"), "each where it was");
}

#[test]
fn a_style_property_keeps_the_rest_of_the_style() {
    let dir = project();
    let mut content = Content::open(dir.path()).unwrap();
    let mut ui = GameUi::default();
    let document = ui.load(&mut content, "UI/Hud.vui").unwrap();
    let bar = ui.find(document, "bar").unwrap();
    let size = |ui: &mut GameUi| {
        ui.frame(0.0, (800, 600), 1.0);
        let ui = ui.ui();
        let [_, _, width, height] = ui.rect(ui.find(ui.root(), "bar").unwrap());
        (width, height)
    };
    assert_eq!(size(&mut ui), (100.0, 10.0), "as its style attribute says");

    ui.ask(bar, UiAsk::SetStyleProperty("width", "40px"));
    assert_eq!(size(&mut ui), (40.0, 10.0), "its height kept");
    ui.ask(bar, UiAsk::SetStyleProperty("width", "50%"));
    assert_eq!(size(&mut ui), (400.0, 10.0));

    // Taken out: the sheets decide it again, and there is no sheet.
    ui.ask(bar, UiAsk::SetStyleProperty("width", ""));
    let (width, height) = size(&mut ui);
    assert!(width != 400.0 && height == 10.0, "{width} by {height}");
}
