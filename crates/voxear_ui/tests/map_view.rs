//! Map views, without a GPU: a minimap and a map made in code, by kind and
//! in markup, each with its picture and overlay, a map with its header and
//! compass, and their settings as markup says.

use voxear_ui::{ElementKind, MapViewKind, MapViewSettings, Style, Ui};

#[test]
fn a_minimap_holds_a_picture_and_an_overlay_and_a_map_its_title_readout_and_compass_too() {
    let mut ui = Ui::new();
    let root = ui.root();
    let hud = ui.panel(root, Style::cover());
    let mini = ui.map_view(hud, MapViewKind::Minimap);
    let large = ui.create(hud, ElementKind::Map, "");
    assert!(ui.has_class(mini, "minimap") && ui.has_class(large, "map"));
    let mut views = ui.map_views();
    views.sort_by_key(|node| *node != mini);
    assert_eq!(views, [mini, large]);

    let parts = ui.map_view_parts(mini).unwrap();
    assert!(ui.has_class(parts.picture, "map-picture") && ui.has_class(parts.overlay, "map-overlay"));
    assert_eq!((parts.title, parts.readout, parts.compass), (None, None, None));
    assert_eq!(ui.map_view_settings(mini), Some(MapViewSettings::of(MapViewKind::Minimap)));
    let parts = ui.map_view_parts(large).unwrap();
    assert!(ui.has_class(parts.title.unwrap(), "map-title") && ui.has_class(parts.readout.unwrap(), "map-readout"));
    assert!(ui.has_class(parts.compass.unwrap(), "map-compass"));

    // The default sheet sizes a minimap, lays its picture over all of it, and fills its parent with a map.
    ui.frame(1280, 720, 1.0);
    assert_eq!(ui.rect(mini)[2..], [220.0, 220.0]);
    assert_eq!(ui.rect(ui.map_view_parts(mini).unwrap().picture), ui.rect(mini));
    assert_eq!(ui.rect(large)[2..], [1280.0, 720.0]);

    // Taken out, a view is forgotten.
    ui.remove(mini);
    assert_eq!(ui.map_views(), [large]);
    assert_eq!(ui.map_view_settings(mini), None);
}

#[test]
fn markup_makes_map_views_with_their_zoom_turning_shape_and_tilt() {
    let mut ui = Ui::new();
    let root = ui.root();
    let markup = r#"<vui><Panel>
        <Minimap name="mini" zoom="8" turns="false" round="true" tilt="90" />
        <Map name="large" zoom="5" />
    </Panel></vui>"#;
    let document = ui.load_markup(root, "Hud.vui", markup, &mut |_| Err("no files".into())).unwrap();
    let mini = ui.find(document, "mini").unwrap();
    let settings = ui.map_view_settings(mini).unwrap();
    assert_eq!((settings.kind, settings.zoom, settings.turns, settings.round), (MapViewKind::Minimap, 8, false, true));
    assert_eq!(settings.tilt, 90.0);
    let large = ui.find(document, "large").unwrap();
    let settings = ui.map_view_settings(large).unwrap();
    assert_eq!((settings.kind, settings.zoom, settings.turns), (MapViewKind::Map, 4, false), "5 is nearest 1:4");

    // Every setting has a name markup and scripts set it by, kept within what a map can show.
    let markup = r#"<vui><Map name="m" fill="1.5" controls="false" min-zoom="2" max-zoom="16" shadows="0" thick="9"
        arrow-at-edge="false" title="false" compass="false" /></vui>"#;
    let document = ui.load_markup(root, "Map.vui", markup, &mut |_| Err("no files".into())).unwrap();
    let set = ui.map_view_settings(ui.find(document, "m").unwrap()).unwrap();
    assert_eq!((set.fill, set.controls, set.zoom_range, set.shadows, set.thick), (1.5, false, [2, 16], 0.0, 9.0));
    assert!(!set.arrow_at_edge && !set.title && !set.compass && set.readout && set.arrow);
    let mut text = MapViewSettings::of(MapViewKind::Map);
    assert_eq!(text.set_all("zoom=9 tilt=5 fill=99 edges=0.25 labels=0 wobble=3 round=maybe"), ["wobble", "round"]);
    assert_eq!((text.zoom, text.tilt, text.fill, text.edges, text.labels), (8, 20.0, 3.0, 0.25, false));
    assert!(ui.load_markup(root, "Bad.vui", r#"<vui><Map fill="wide" /></vui>"#, &mut |_| Err("no".into())).is_err());
    // Written out and read back, settings are the same.
    let mut read = MapViewSettings::of(MapViewKind::Map);
    assert!(read.set_all(&set.to_text()).is_empty());
    assert_eq!(read, set);
    for name in MapViewSettings::NAMES {
        let value = if text.clone().set(name, "1") { "1" } else { "true" };
        assert!(text.set(name, value), "{name}");
    }

    // Settings changed in code keep the view's kind.
    ui.set_map_view_settings(large, MapViewSettings { zoom: 16, ..MapViewSettings::of(MapViewKind::Minimap) });
    let settings = ui.map_view_settings(large).unwrap();
    assert_eq!((settings.kind, settings.zoom, settings.turns), (MapViewKind::Map, 16, true));
}
