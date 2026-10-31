//! Measuring text for layout, and walking the laid-out tree into quads.

use parley::{Alignment, AlignmentOptions, FontContext, Layout, LayoutContext, PositionedLayoutItem};
use taffy::{AvailableSpace, Display, LayoutInput, LayoutOutput, Size, TaffyTree};

use crate::{
    Animated, Color, Overflow, Quad, Style, TextAlign, animation,
    atlas::GlyphAtlas,
    controls::Control,
    scroll::{BAR, BAR_INSET, thumb},
    tree::{Element, Kind},
};

/// Something drawn on an element with [`Ui::set_shapes`](crate::Ui::set_shapes),
/// in logical pixels from the top left of its content box.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Shape {
    /// A straight line with round ends.
    Line { from: [f32; 2], to: [f32; 2], width: f32, color: Color },
    /// A filled circle, maybe with a border inside its edge.
    Circle { center: [f32; 2], radius: f32, fill: Color, border: Option<(f32, Color)> },
}

/// How big an icon is when nothing sizes it.
const ICON: f32 = 14.0;
/// How wide an icon's lines are, whatever its size.
const ICON_STROKE: f32 = 1.5;

/// How big a leaf is: its text, broken to fit, or nothing.
pub(crate) fn measure(
    input: LayoutInput,
    element: Option<&mut Element>,
    style: &taffy::Style,
    fonts: &mut FontContext,
    layouts: &mut LayoutContext<()>,
    scale: f32,
) -> LayoutOutput {
    let Some(element) = element else {
        return taffy::compute_leaf_layout(input, style, |_, _| 0.0, |_, _| Size::ZERO);
    };
    let text_style = element.text_style;
    if element.kind == Kind::Icon {
        return taffy::compute_leaf_layout(
            input,
            style,
            |_, _| 0.0,
            |known, _| Size { width: known.width.unwrap_or(ICON), height: known.height.unwrap_or(ICON) },
        );
    }
    if let Some(Control::TextField(edit)) = &mut element.control {
        return taffy::compute_leaf_layout(
            input,
            style,
            |_, _| 0.0,
            |known, available| {
                let width = known.width.or(match available.width {
                    AvailableSpace::Definite(width) => Some(width),
                    _ => None,
                });
                edit.sync(text_style, scale, width.map(|w| w * scale));
                let height = edit.editor.layout(fonts, layouts).height() / scale;
                // Like CSS's inputs: a set width when there is room, but able to shrink.
                let natural = if available.width == AvailableSpace::MinContent { FIELD_MIN_WIDTH } else { FIELD_WIDTH };
                Size {
                    width: known.width.unwrap_or(natural),
                    height: known.height.unwrap_or(height.max(text_style.size * 1.25)),
                }
            },
        );
    }
    let Some(text) = element.text.as_mut() else {
        return taffy::compute_leaf_layout(input, style, |_, _| 0.0, |_, _| Size::ZERO);
    };
    taffy::compute_leaf_layout(
        input,
        style,
        |_, _| 0.0,
        |known, available| {
            let max = known.width.or(match available.width {
                AvailableSpace::Definite(width) => Some(width),
                AvailableSpace::MinContent => Some(0.0),
                AvailableSpace::MaxContent => None,
            });
            let layout = text.shaped(fonts, layouts, scale);
            layout.break_all_lines(max.map(|w| w * scale));
            Size {
                width: known.width.unwrap_or(layout.width() / scale),
                height: known.height.unwrap_or(layout.height() / scale),
            }
        },
    )
}

/// How wide a text field is when nothing sets its width, and the least it
/// shrinks to.
const FIELD_WIDTH: f32 = 160.0;
const FIELD_MIN_WIDTH: f32 = 40.0;

/// Walks the laid-out tree into quads, parents before children.
pub(crate) struct Draw<'a> {
    pub tree: &'a mut TaffyTree<Element>,
    pub atlas: &'a mut GlyphAtlas,
    pub scale: f32,
    pub quads: &'a mut Vec<Quad>,
    /// The focused element, and whether its caret shows this frame.
    pub focused: Option<taffy::NodeId>,
    pub caret_on: bool,
    /// The UI's clock, for transitions.
    pub time: f32,
}

impl Draw<'_> {
    /// Draws `node` at `parent`'s position, faded by the opacity of the
    /// elements it is in and kept within `clip` (physical pixels).
    pub fn node(&mut self, node: taffy::NodeId, parent: (f32, f32), opacity: f32, clip: [f32; 4]) {
        if self.tree.style(node).is_ok_and(|s| s.display == Display::None) {
            return;
        }
        let layout = *self.tree.layout(node).expect("laid out");
        let at = (parent.0 + layout.location.x, parent.1 + layout.location.y);
        let [x0, y0] = self.snap(at.0, at.1);
        let [x1, y1] = self.snap(at.0 + layout.size.width, at.1 + layout.size.height);
        let inset = (layout.border.left + layout.padding.left, layout.border.top + layout.padding.top);
        let origin = self.snap(at.0 + inset.0, at.1 + inset.1);
        let inner = layout.size.width - inset.0 - layout.border.right - layout.padding.right;
        let inner_height = layout.size.height - inset.1 - layout.border.bottom - layout.padding.bottom;
        let outside = x1 <= clip[0] || y1 <= clip[1] || x0 >= clip[2] || y0 >= clip[3];
        let Some(element) = self.tree.get_node_context_mut(node) else { return };
        let mut look = Look::of(&element.computed);
        let mut text_color = element.text_style.color.linear_premultiplied();
        let mut own_opacity = element.computed.opacity;
        if !element.animations.is_empty() {
            look.background = animation::showing(element, Animated::Background, self.time);
            look.border_color = animation::showing(element, Animated::BorderColor, self.time);
            text_color = animation::showing(element, Animated::Color, self.time);
            own_opacity = animation::showing(element, Animated::Opacity, self.time)[0];
            animation::settle(element, self.time);
        }
        let opacity = opacity * own_opacity;
        let (overflow, scroll, scroll_max, bar) =
            (element.computed.overflow, element.scroll, element.scroll_max, element.computed.scrollbar);
        if !outside {
            let rect = [x0, y0, x1 - x0, y1 - y0];
            if let Some(shadow) = &element.computed.shadow {
                shadow_quad(shadow, &look, rect, self.scale, opacity, clip, self.quads);
            }
            shape_quad(&look, rect, self.scale, opacity, clip, self.quads);
            let content = [origin[0], origin[1], inner * self.scale, inner_height * self.scale];
            shape_quads(&element.shapes, content, self.scale, opacity, clip, self.quads);
            if let Some(icon) = element.icon {
                let color = text_color.map(|c| c * opacity);
                icon_quad(icon, content, self.scale, color, clip, (self.atlas, self.quads));
            }
            if let Some(Control::TextField(_)) = &element.control {
                let content =
                    [origin[0], origin[1], origin[0] + inner * self.scale, origin[1] + inner_height * self.scale];
                let clip = [
                    clip[0].max(content[0]),
                    clip[1].max(content[1]),
                    clip[2].min(content[2]),
                    clip[3].min(content[3]),
                ];
                let focused = self.focused == Some(node);
                let field = Field { focused, caret: focused && self.caret_on, opacity, clip };
                draw_field(element, origin, field, self.atlas, self.quads);
            }
            let color = text_color.map(|c| c * opacity);
            if let Some(text) = &mut element.text
                && let Some((_, shaped)) = &mut text.shaped
            {
                // Broken again at the width it ended up, which the last
                // measure may not have been.
                shaped.break_all_lines(Some((inner * self.scale).ceil()));
                let alignment = match element.text_style.align {
                    TextAlign::Start => Alignment::Start,
                    TextAlign::Center => Alignment::Center,
                    TextAlign::End => Alignment::End,
                };
                shaped.align(alignment, AlignmentOptions::default());
                // Centred down a box taller than the text, such as a button
                // with a set height, as Unity's middle alignment does.
                let spare = (inner_height * self.scale - shaped.height()).max(0.0);
                let origin = [origin[0], origin[1] + (spare / 2.0).round()];
                glyph_quads(shaped, origin, color, clip, self.atlas, self.quads);
            }
        }
        // What it clips cannot show when it is itself out of sight.
        if opacity <= 0.0 || (outside && overflow != Overflow::Visible) {
            return;
        }
        let padding_box = [
            at.0 + layout.border.left,
            at.1 + layout.border.top,
            layout.size.width - layout.border.left - layout.border.right,
            layout.size.height - layout.border.top - layout.border.bottom,
        ];
        let child_clip = if overflow == Overflow::Visible {
            clip
        } else {
            let [l, t] = self.snap(padding_box[0], padding_box[1]);
            let [r, b] = self.snap(padding_box[0] + padding_box[2], padding_box[1] + padding_box[3]);
            [clip[0].max(l), clip[1].max(t), clip[2].min(r), clip[3].min(b)]
        };
        let origin = (at.0 - scroll[0], at.1 - scroll[1]);
        for child in self.tree.children(node).unwrap_or_default() {
            self.node(child, origin, opacity, child_clip);
        }
        if overflow == Overflow::Scroll
            && let Some((top, length)) = thumb(padding_box[3], scroll_max[1], scroll[1])
        {
            let x = padding_box[0] + padding_box[2] - BAR - BAR_INSET;
            let [l, t] = self.snap(x, padding_box[1] + top);
            let [r, b] = self.snap(x + BAR, padding_box[1] + top + length);
            let look = Look { background: bar.linear_premultiplied(), radius: BAR / 2.0, ..Look::default() };
            shape_quad(&look, [l, t, r - l, b - t], self.scale, opacity, child_clip, self.quads);
        }
    }

    /// A logical point on the physical pixel grid.
    fn snap(&self, x: f32, y: f32) -> [f32; 2] {
        [(x * self.scale).round(), (y * self.scale).round()]
    }
}

/// The colours of a box as drawn this frame, linear and premultiplied:
/// its style's, or what its transitions show.
#[derive(Default)]
struct Look {
    background: [f32; 4],
    border: f32,
    border_color: [f32; 4],
    dashed: bool,
    radius: f32,
}

impl Look {
    fn of(style: &Style) -> Self {
        let (border, border_color) = style.border.map_or((0.0, [0.0; 4]), |(w, c)| (w, c.linear_premultiplied()));
        Self {
            background: style.background.map_or([0.0; 4], |c| c.linear_premultiplied()),
            border,
            border_color,
            dashed: style.dashed,
            radius: style.radius,
        }
    }
}

/// A box's shadow: its rounded box moved, grown and blurred, outside it.
fn shadow_quad(
    shadow: &crate::Shadow,
    look: &Look,
    rect: [f32; 4],
    scale: f32,
    opacity: f32,
    clip: [f32; 4],
    quads: &mut Vec<Quad>,
) {
    let color = shadow.color.linear_premultiplied().map(|c| c * opacity);
    if color[3] <= 0.0 {
        return;
    }
    let spread = shadow.spread * scale;
    let caster = [
        rect[0] + shadow.x * scale - spread,
        rect[1] + shadow.y * scale - spread,
        rect[2] + spread * 2.0,
        rect[3] + spread * 2.0,
    ];
    let sigma = shadow.blur * scale / 2.0;
    // Three deviations out, a Gaussian has nothing left to show.
    let reach = (sigma * 3.0).ceil();
    quads.push(Quad {
        rect: [caster[0] - reach, caster[1] - reach, caster[2] + reach * 2.0, caster[3] + reach * 2.0],
        uv: caster,
        color,
        border_color: rect,
        clip,
        radius: (look.radius * scale + spread).max(0.0),
        border: sigma,
        kind: Quad::SHADOW,
        ..Quad::default()
    });
}

/// Lines and circles in a content box (physical pixels: left, top, width
/// and height), their coordinates scaled from logical ones.
fn shape_quads(shapes: &[Shape], content: [f32; 4], scale: f32, opacity: f32, clip: [f32; 4], quads: &mut Vec<Quad>) {
    let at = |p: [f32; 2]| [content[0] + p[0] * scale, content[1] + p[1] * scale];
    for shape in shapes {
        match *shape {
            Shape::Line { from, to, width, color } => {
                let (a, b, width) = (at(from), at(to), width * scale);
                let pad = width / 2.0 + 1.0;
                let (left, top) = (a[0].min(b[0]) - pad, a[1].min(b[1]) - pad);
                let (right, bottom) = (a[0].max(b[0]) + pad, a[1].max(b[1]) + pad);
                quads.push(Quad {
                    rect: [left, top, right - left, bottom - top],
                    uv: [a[0], a[1], b[0], b[1]],
                    color: color.linear_premultiplied().map(|c| c * opacity),
                    clip,
                    border: width,
                    kind: Quad::LINE,
                    ..Quad::default()
                });
            }
            Shape::Circle { center, radius, fill, border } => {
                let (c, r) = (at(center), radius * scale);
                let look = Look {
                    background: fill.linear_premultiplied(),
                    border: border.map_or(0.0, |(w, _)| w),
                    border_color: border.map_or([0.0; 4], |(_, c)| c.linear_premultiplied()),
                    dashed: false,
                    radius,
                };
                shape_quad(&look, [c[0] - r, c[1] - r, r * 2.0, r * 2.0], scale, opacity, clip, quads);
            }
        }
    }
}

/// An icon centred in a content box, as big as fits, on whole pixels.
fn icon_quad(
    name: &str,
    content: [f32; 4],
    scale: f32,
    color: [f32; 4],
    clip: [f32; 4],
    draw: (&mut GlyphAtlas, &mut Vec<Quad>),
) {
    let (atlas, quads) = draw;
    let px = content[2].min(content[3]).round();
    if color[3] <= 0.0 || px < 1.0 {
        return;
    }
    let Some(found) = atlas.icon(name, px as u32, ICON_STROKE * scale) else { return };
    let size = atlas.size() as f32;
    let left = (content[0] + (content[2] - px) / 2.0).round();
    let top = (content[1] + (content[3] - px) / 2.0).round();
    let (x, y) = (found.x as f32, found.y as f32);
    quads.push(Quad {
        rect: [left, top, px, px],
        uv: [x / size, y / size, (x + px) / size, (y + px) / size],
        color,
        clip,
        kind: Quad::GLYPH,
        ..Quad::default()
    });
}

fn shape_quad(look: &Look, rect: [f32; 4], scale: f32, opacity: f32, clip: [f32; 4], quads: &mut Vec<Quad>) {
    let border = look.border > 0.0 && look.border_color[3] > 0.0;
    if (look.background[3] <= 0.0 && !border) || opacity <= 0.0 {
        return;
    }
    let border = if border { (look.border * scale).round().max(1.0) } else { 0.0 };
    quads.push(Quad {
        rect,
        color: look.background.map(|c| c * opacity),
        border_color: look.border_color.map(|c| c * opacity),
        clip,
        radius: look.radius * scale,
        border,
        kind: Quad::SHAPE,
        flags: if look.dashed { Quad::DASHED } else { 0 },
        ..Quad::default()
    });
}

/// Every glyph of a shaped text, on whole pixels: hinted glyphs are drawn
/// where the hinting expected them.
fn glyph_quads(
    layout: &Layout<()>,
    origin: [f32; 2],
    color: [f32; 4],
    clip: [f32; 4],
    atlas: &mut GlyphAtlas,
    quads: &mut Vec<Quad>,
) {
    if color[3] <= 0.0 {
        return;
    }
    let size = atlas.size() as f32;
    for line in layout.lines() {
        for item in line.items() {
            let PositionedLayoutItem::GlyphRun(glyph_run) = item else { continue };
            let run = glyph_run.run();
            let font = run.font();
            let font = (font.data.data(), font.index, font.data.id());
            let coords: Vec<i16> = run.normalized_coords().iter().map(|c| c.to_bits()).collect();
            for glyph in glyph_run.positioned_glyphs() {
                let Some(found) = atlas.glyph(font, glyph.id as u16, run.font_size(), &coords) else { continue };
                let pen = [(origin[0] + glyph.x).round(), (origin[1] + glyph.y).round()];
                let (x, y) = (found.x as f32, found.y as f32);
                let (w, h) = (found.width as f32, found.height as f32);
                quads.push(Quad {
                    rect: [pen[0] + found.left as f32, pen[1] - found.top as f32, w, h],
                    uv: [x / size, y / size, (x + w) / size, (y + h) / size],
                    color,
                    clip,
                    kind: Quad::GLYPH,
                    ..Quad::default()
                });
            }
        }
    }
}

/// How a text field is drawn this frame.
struct Field {
    focused: bool,
    caret: bool,
    opacity: f32,
    /// Its content box, within whatever clips it.
    clip: [f32; 4],
}

/// A text field's selection, text (or placeholder) and caret, from the top
/// left of its content box.
fn draw_field(element: &mut Element, origin: [f32; 2], field: Field, atlas: &mut GlyphAtlas, quads: &mut Vec<Quad>) {
    let Some(Control::TextField(edit)) = &mut element.control else { return };
    let style = &element.computed;
    let fade = |color: crate::Color| color.linear_premultiplied().map(|c| c * field.opacity);
    let at = [origin[0] - edit.scroll.round(), origin[1] + edit.top];
    let solid =
        |rect: [f32; 4], color: [f32; 4]| Quad { rect, color, clip: field.clip, kind: Quad::SHAPE, ..Quad::default() };
    if field.focused {
        for (bounds, _) in edit.editor.selection_geometry() {
            let rect = [
                at[0] + bounds.x0 as f32,
                at[1] + bounds.y0 as f32,
                (bounds.x1 - bounds.x0) as f32,
                (bounds.y1 - bounds.y0) as f32,
            ];
            quads.push(solid(rect, fade(style.selection)));
        }
    }
    let empty = edit.editor.raw_text().is_empty();
    if empty && !edit.placeholder.content.is_empty() {
        if let Some((_, layout)) = &edit.placeholder.shaped {
            glyph_quads(layout, at, fade(edit.placeholder.style.color), field.clip, atlas, quads);
        }
    } else if let Some(layout) = edit.editor.try_layout() {
        glyph_quads(layout, at, fade(element.text_style.color), field.clip, atlas, quads);
    }
    if field.caret
        && let Some(bounds) = edit.editor.cursor_geometry(1.0)
    {
        let caret = style.caret.unwrap_or(element.text_style.color);
        let rect = [
            (at[0] + bounds.x0 as f32).round(),
            at[1] + bounds.y0 as f32,
            1.0f32.max((bounds.x1 - bounds.x0) as f32),
            (bounds.y1 - bounds.y0) as f32,
        ];
        quads.push(solid(rect, fade(caret)));
    }
}
