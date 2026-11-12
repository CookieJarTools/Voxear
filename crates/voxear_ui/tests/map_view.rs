//! Map views, without a GPU: a minimap and a map made in code, by kind and
//! in markup, each with its picture and overlay, a map with its header and
//! compass, and their settings as markup says.

use voxear_ui::{ElementKind, MapViewKind, MapViewSettings, Style, Ui};

#[test]
fn a_minimap_and_a_map_hold_their_parts_sized_as_their_design_and_scaled_with_the_screen() {
    let mut ui = Ui::new();
    let root = ui.root();
    let hud = ui.panel(root, Style::cover());
    let mini = ui.map_view(hud, MapViewKind::Minimap);
    let large = ui.create(hud, ElementKind::Map, "");
    assert!(ui.has_class(mini, "minimap") && ui.has_class(large, "map"));
    let mut views = ui.map_views();
    views.sort_by_key(|node| *node != mini);
    assert_eq!(views, [mini, large]);

    // A minimap: a window on the ground with a compass and its zoom, and one strip that says where and how it is.
    let small = ui.map_view_parts(mini).unwrap();
    assert!(ui.has_class(small.picture, "map-picture") && ui.has_class(small.overlay, "map-overlay"));
    assert_eq!(small.region, small.environment, "one strip says both");
    assert_eq!(
        (small.zoom_in, small.tag, small.letters[1]),
        (None, None, None),
        "no buttons, no name tag, north alone"
    );
    assert_eq!(ui.map_view_settings(mini), Some(MapViewSettings::of(MapViewKind::Minimap)));
    // A map: a panel for where, a strip for the air, a compass over buttons to zoom, and a name tag.
    let parts = ui.map_view_parts(large).unwrap();
    assert!(ui.has_class(parts.title, "map-title") && ui.has_class(parts.region, "map-region"));
    assert!(ui.has_class(parts.environment, "map-environment") && ui.has_class(parts.compass, "map-compass"));
    assert!(parts.zoom_in.is_some() && parts.zoom_out.is_some() && parts.tag.is_some());
    assert!(parts.letters.iter().all(Option::is_some));

    // On a screen as high as the design's, each part is where and as large as the design has it.
    ui.frame(1920, 1080, 1.0);
    assert_eq!(ui.rect(large)[2..], [1920.0, 1080.0], "a map covers what it is in");
    assert_eq!(ui.rect(mini)[2], 380.0);
    let window = ui.rect(small.picture);
    assert!(window[3] == 254.0 && window[2] > 360.0 && window[2] < 370.0, "{window:?}");
    assert_eq!(ui.rect(parts.region)[..2], [48.0, 48.0]);
    let (nav, compass) = (ui.rect(parts.zoom), ui.rect(parts.compass));
    assert_eq!((compass[2], compass[3], nav[2]), (168.0, 168.0, 68.0));
    assert!((compass[0] + compass[2] - (1920.0 - 48.0)).abs() < 0.5 && (nav[1] + nav[3] - (1080.0 - 48.0)).abs() < 0.5);
    assert_eq!(ui.rect(parts.zoom_in.unwrap())[3], 56.0);

    // Scaled to a screen two thirds as high, every size is two thirds.
    assert!(ui.size_map_view(large, 2.0 / 3.0) && ui.size_map_view(mini, 2.0 / 3.0));
    assert!(!ui.size_map_view(large, 2.0 / 3.0), "sized so already");
    ui.frame(1280, 720, 1.0);
    assert_eq!(ui.rect(parts.region)[..2], [32.0, 32.0]);
    assert!((ui.rect(mini)[2] - 253.33).abs() < 0.01 && (ui.rect(parts.compass)[2] - 112.0).abs() < 0.01);
    assert_eq!(ui.map_view_scale(large), 2.0 / 3.0);

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
