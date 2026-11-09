//! The UI tree without a GPU: layout, text measuring, and the draw list.
//! Text uses the system's fonts, so sizes are checked loosely.

use voxear_ui::{Color, Quad, Style, TextStyle, Ui};

const WIDTH: u32 = 800;
const HEIGHT: u32 = 600;

fn shapes(quads: &[Quad]) -> Vec<&Quad> {
    quads.iter().filter(|q| q.kind == Quad::SHAPE).collect()
}

fn glyphs(quads: &[Quad]) -> usize {
    quads.iter().filter(|q| q.kind == Quad::GLYPH).count()
}

#[test]
fn panels_lay_out_like_flexbox() {
    let mut ui = Ui::new();
    let root = ui.root();
    let panel = ui.panel(root, Style::column().at(10.0, 20.0).padding(8.0).gap(4.0).width(200.0));
    let first = ui.panel(panel, Style::row().background(Color::WHITE));
    let second = ui.panel(panel, Style::row().background(Color::WHITE));
    ui.label(first, "first", TextStyle::default());
    ui.label(second, "second", TextStyle::default());
    ui.frame(WIDTH, HEIGHT, 1.0);

    let [x, y, w, _] = ui.rect(panel);
    assert_eq!((x, y, w), (10.0, 20.0, 200.0));
    let a = ui.rect(first);
    let b = ui.rect(second);
    assert_eq!((a[0], a[1], a[2]), (18.0, 28.0, 184.0), "inside the padding, stretched across");
    assert!(a[3] > 10.0, "as tall as a line of text, got {}", a[3]);
    assert_eq!(b[1], a[1] + a[3] + 4.0, "below the first, after the gap");
}

#[test]
fn labels_measure_their_text_and_wrap() {
    let mut ui = Ui::new();
    let root = ui.root();
    // A row, since a column stretches its children to the widest.
    let wide = ui.panel(root, Style::row().at(0.0, 0.0));
    let short = ui.label(wide, "Hi", TextStyle::default());
    let long = ui.label(wide, "A much longer line of text than that", TextStyle::default());
    let narrow = ui.panel(root, Style::column().at(0.0, 100.0).width(60.0));
    let wrapped = ui.label(narrow, "A much longer line of text than that", TextStyle::default());
    ui.frame(WIDTH, HEIGHT, 1.0);

    let (short, long, wrapped) = (ui.rect(short), ui.rect(long), ui.rect(wrapped));
    assert!(short[2] > 5.0 && long[2] > short[2] * 4.0, "{short:?} {long:?}");
    assert!(wrapped[2] <= 60.0, "kept to its parent, got {wrapped:?}");
    assert!(wrapped[3] > long[3] * 2.5, "broken over several lines, got {wrapped:?}");
}

#[test]
fn frames_draw_shapes_then_glyphs_and_upload_the_atlas_once() {
    let mut ui = Ui::new();
    let root = ui.root();
    let panel = ui.panel(root, Style::column().at(4.0, 4.0).padding(6.0).background(Color::hex(0x111216)).radius(8.0));
    let label = ui.label(panel, "Voxear", TextStyle::default().size(14.0));

    let frame = ui.frame(WIDTH, HEIGHT, 1.0);
    let shapes = shapes(&frame.quads);
    assert_eq!(shapes.len(), 1, "a label with no background draws only glyphs");
    assert_eq!(shapes[0].rect[..2], [4.0, 4.0]);
    assert_eq!(shapes[0].radius, 8.0);
    assert_eq!(glyphs(&frame.quads), 6);
    assert_eq!(frame.quads[0].kind, Quad::SHAPE, "the panel is under its text");
    let atlas = frame.atlas.expect("the first frame has the atlas");
    assert!(atlas.pixels.iter().any(|&p| p > 200), "glyphs were rasterised");
    for glyph in frame.quads.iter().filter(|q| q.kind == Quad::GLYPH) {
        assert_eq!(glyph.rect[0].fract(), 0.0, "glyphs sit on whole pixels");
        assert_eq!(glyph.rect[1].fract(), 0.0, "glyphs sit on whole pixels");
    }

    let same = ui.frame(WIDTH, HEIGHT, 1.0).atlas.unwrap();
    assert!(std::sync::Arc::ptr_eq(&same, &atlas), "nothing new, so the same image, uncopied");
    ui.set_text(label, "Voxear!");
    let frame = ui.frame(WIDTH, HEIGHT, 1.0);
    assert_eq!(glyphs(&frame.quads), 7);
    assert!(frame.atlas.unwrap().version > atlas.version, "the new glyph made a new version");
}

#[test]
fn hidden_elements_draw_nothing_and_take_no_space() {
    let mut ui = Ui::new();
    let root = ui.root();
    let column = ui.panel(root, Style::column().at(0.0, 0.0));
    let hidden = ui.panel(column, Style::column().background(Color::WHITE).padding(10.0));
    ui.label(hidden, "hidden", TextStyle::default());
    let shown = ui.label(column, "shown", TextStyle::default());

    ui.set_visible(hidden, false);
    let frame = ui.frame(WIDTH, HEIGHT, 1.0);
    assert!(shapes(&frame.quads).is_empty());
    assert_eq!(glyphs(&frame.quads), 5);
    assert_eq!(ui.rect(shown)[1], 0.0);

    ui.set_visible(hidden, true);
    let frame = ui.frame(WIDTH, HEIGHT, 1.0);
    assert_eq!(shapes(&frame.quads).len(), 1);
    assert!(ui.rect(shown)[1] > 20.0);

    ui.remove(hidden);
    let frame = ui.frame(WIDTH, HEIGHT, 1.0);
    assert!(shapes(&frame.quads).is_empty());
    assert_eq!(ui.rect(shown)[1], 0.0);
}

#[test]
fn display_scale_draws_at_physical_size() {
    let mut ui = Ui::new();
    let root = ui.root();
    let panel = ui.panel(root, Style::column().at(10.0, 10.0).width(100.0).padding(4.0).border(1.0, Color::WHITE));
    ui.label(panel, "Scale", TextStyle::default());

    let one = ui.frame(WIDTH, HEIGHT, 1.0);
    let two = ui.frame(WIDTH * 2, HEIGHT * 2, 2.0);
    let (a, b) = (shapes(&one.quads)[0], shapes(&two.quads)[0]);
    assert_eq!(b.rect, [20.0, 20.0, 200.0, b.rect[3]]);
    assert_eq!(b.border, 2.0);
    assert!((b.rect[3] - a.rect[3] * 2.0).abs() <= 2.0, "{a:?} {b:?}");
    let height = |quads: &[Quad]| quads.iter().filter(|q| q.kind == Quad::GLYPH).map(|q| q.rect[3]).fold(0.0, f32::max);
    let (small, large) = (height(&one.quads), height(&two.quads));
    assert!(large > small * 1.7, "glyphs rasterised at the larger size: {small} then {large}");
    assert!(two.atlas.unwrap().version > one.atlas.unwrap().version, "new sizes were rasterised");
}

#[test]
fn colours_are_linear_and_premultiplied() {
    assert_eq!(Color::WHITE.linear_premultiplied(), [1.0, 1.0, 1.0, 1.0]);
    assert_eq!(Color::TRANSPARENT.linear_premultiplied(), [0.0; 4]);
    let [r, g, b, a] = Color::hex(0x808080).with_alpha(0x80).linear_premultiplied();
    assert!((a - 0.502).abs() < 0.001);
    assert!((r - 0.2158 * a).abs() < 0.001, "sRGB 50% grey is about 21.6% linear, got {r}");
    assert_eq!((r, r), (g, b));
    assert_eq!(Color::hex(0x2a2b30), Color::rgb(0x2a, 0x2b, 0x30));
}

#[test]
fn a_see_through_element_cuts_away_what_was_drawn_under_it() {
    let mut ui = Ui::new();
    let root = ui.root();
    ui.panel(root, Style::column().at(0.0, 0.0).width(300.0).height(200.0).background(Color::WHITE));
    let window = ui.panel(root, Style::column().at(100.0, 50.0).width(100.0).height(100.0));
    let view = ui.panel(window, Style::column().width(100.0).height(100.0));
    ui.set_see_through(view, true);
    ui.panel(view, Style::column().width(20.0).height(20.0).background(Color::WHITE));
    let frame = ui.frame(WIDTH, HEIGHT, 1.0);

    let under_parts: Vec<&Quad> = shapes(&frame.quads).into_iter().filter(|q| q.rect[2] == 300.0).collect();
    assert_eq!(under_parts.len(), 4, "the panel under it is drawn as the four bands around it");
    let clips: Vec<[f32; 4]> = under_parts.iter().map(|q| q.clip).collect();
    assert!(clips.contains(&[0.0, 0.0, 300.0, 50.0]), "above it, {clips:?}");
    assert!(clips.contains(&[0.0, 150.0, 300.0, 200.0]), "below it, {clips:?}");
    assert!(clips.contains(&[0.0, 50.0, 100.0, 150.0]), "left of it, {clips:?}");
    assert!(clips.contains(&[200.0, 50.0, 300.0, 150.0]), "right of it, {clips:?}");
    assert!(shapes(&frame.quads).iter().any(|q| q.rect == [100.0, 50.0, 20.0, 20.0]), "what is in it still draws");

    ui.set_see_through(view, false);
    let frame = ui.frame(WIDTH, HEIGHT, 1.0);
    assert_eq!(shapes(&frame.quads).iter().filter(|q| q.rect[2] == 300.0).count(), 1);
}

/// A background drawn by a shader: its quad says which and where its values
/// start, read from the element's custom properties or the shader's
/// defaults; a shader there is none of draws the background plain, once
/// warned of.
#[test]
fn a_shaded_background_names_its_shader_and_carries_its_values() {
    use voxear_ui::{BUILTIN_UI_SHADERS, StyleSheet, UiShader, UiShaderProperty, UiShaderValue, UiShaders};

    let mut ui = Ui::new();
    let root = ui.root();
    let sheet = ".busy { width: 100px; height: 20px; shader: stripes; --spacing: 20; }
        .glow { width: 50px; height: 20px; background-color: #ff0000; shader: url(\"Shaders/Glow.shader\"); }
        .lost { width: 50px; height: 20px; background-color: #00ff00; shader: missing; }";
    ui.add_sheet(root, StyleSheet::parse("Test.vss", sheet));
    let busy = ui.panel(root, Style::default());
    ui.add_class(busy, "busy");
    let glow = ui.panel(root, Style::default());
    ui.add_class(glow, "glow");
    let lost = ui.panel(root, Style::default());
    ui.add_class(lost, "lost");
    let mut shaders = UiShaders::default();
    let strength = UiShaderProperty { name: "strength".into(), value: UiShaderValue::Number, default: [0.5, 0.0, 0.0, 0.0] };
    shaders.insert("Shaders/Glow.shader", UiShader { number: 9, properties: vec![strength] });
    let ui_shaders = std::rc::Rc::new(shaders);
    ui.set_shaders(ui_shaders.clone());
    let frame = ui.frame(WIDTH, HEIGHT, 2.0);

    let shaded: Vec<&Quad> = frame.quads.iter().filter(|q| q.kind == Quad::SHAPE && q.flags & Quad::SHADED != 0).collect();
    assert_eq!(shaded.len(), 2, "the stripes and the glow, though the stripes' background is clear");
    let number = |q: &Quad| (q.flags >> Quad::SHADER_SHIFT) & 0xffff;
    let first = |q: &Quad| q.uv[0].to_bits() as usize;
    assert_eq!(number(shaded[0]), BUILTIN_UI_SHADERS + 1, "stripes");
    assert_eq!(frame.values[first(shaded[0]) + 1][0], 20.0, "its spacing, from --spacing");
    assert_eq!(frame.values[first(shaded[0]) + 3][0], 45.0, "its angle, the default");
    assert_eq!(shaded[0].uv[1], 2.0, "with the display scale");
    assert_eq!(number(shaded[1]), 9, "the project's shader, by its path");
    assert_eq!(frame.values[first(shaded[1])][0], 0.5);
    assert_eq!(shaded[1].color, [1.0, 0.0, 0.0, 1.0], "its background, for the shader to use");
    let plain = frame.quads.iter().filter(|q| q.kind == Quad::SHAPE && q.color == [0.0, 1.0, 0.0, 1.0]).count();
    assert_eq!(plain, 1, "an unknown shader's background is drawn plain");
    assert!(ui.take_warnings().iter().any(|w| w.contains("no UI shader 'missing'")));
    ui.set_shaders(ui_shaders);
    ui.frame(WIDTH, HEIGHT, 2.0);
    assert!(ui.take_warnings().iter().all(|w| !w.contains("no UI shader")), "said once, not every frame");
}
