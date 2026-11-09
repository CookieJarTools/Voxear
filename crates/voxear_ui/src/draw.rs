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
    /// A filled rectangle, left, top, width and height, with rounded
    /// corners, as a timeline's bars are drawn.
    Rect { rect: [f32; 4], radius: f32, fill: Color },
    /// One of the icons ([`ICONS`](crate::ICONS)), `size` pixels square,
    /// centred on a point, as the Scene view marks Objs.
    Icon { name: &'static str, center: [f32; 2], size: f32, color: Color },
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
    layouts: &mut LayoutContext<u16>,
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
            // The available width, not the known one: taffy gives the known width with the
            // padding still in it, and text broken to that is a line short in a padded box.
            let max = match available.width {
                AvailableSpace::Definite(width) => Some(width),
                AvailableSpace::MinContent => Some(0.0),
                AvailableSpace::MaxContent => None,
            };
            let layout = text.shaped(fonts, layouts, scale);
            layout.break_all_lines(max.filter(|_| text_style.wrap).map(|w| w * scale));
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

/// An element put off to be drawn over everything: its `z-index`, then
/// where, how faded and how clipped it would have been drawn.
pub(crate) type Raised = (i32, taffy::NodeId, (f32, f32), f32, [f32; 4]);

/// Walks the laid-out tree into quads, parents before children, then the
/// elements raised with `z-index` over everything.
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
    /// Raised elements met on the way, to draw last.
    pub raised: Vec<Raised>,
    /// Drawing the raised elements now, so they are not put off again.
    pub lifting: bool,
    /// Elements shown in desktop windows, which only their own windows draw.
    pub skip: Vec<taffy::NodeId>,
    /// The shaders backgrounds can be drawn with, the values the shaded
    /// ones read, and the names styles gave that no shader has.
    pub shaders: &'a crate::UiShaders,
    pub values: &'a mut Vec<[f32; 4]>,
    pub unknown_shaders: &'a mut Vec<String>,
}

impl Draw<'_> {
    /// Draws the whole tree from `root`, then what is raised over it, the
    /// lowest `z-index` first.
    pub fn all(&mut self, root: taffy::NodeId) {
        self.subtree(root, (0.0, 0.0));
    }

    /// Draws `node` and what is in it, its parent's place on screen at
    /// `parent`, unclipped by what it is in, then what is raised in it: a
    /// desktop window's element, drawn alone.
    pub fn subtree(&mut self, node: taffy::NodeId, parent: (f32, f32)) {
        self.skip.retain(|&skipped| skipped != node);
        self.node(node, parent, 1.0, Quad::UNCLIPPED);
        let mut raised = std::mem::take(&mut self.raised);
        raised.sort_by_key(|(z, ..)| *z);
        self.lifting = true;
        for (_, node, parent, opacity, clip) in raised {
            self.node(node, parent, opacity, clip);
        }
        self.lifting = false;
    }

    /// Draws `node` at `parent`'s position, faded by the opacity of the
    /// elements it is in and kept within `clip` (physical pixels).
    pub fn node(&mut self, node: taffy::NodeId, parent: (f32, f32), opacity: f32, clip: [f32; 4]) {
        if self.tree.style(node).is_ok_and(|s| s.display == Display::None) || self.skip.contains(&node) {
            return;
        }
        let z = self.tree.get_node_context(node).map_or(0, |e| e.computed.z_index);
        if z > 0 && !self.lifting {
            self.raised.push((z, node, parent, opacity, clip));
            return;
        }
        let layout = *self.tree.layout(node).expect("laid out");
        let shift = self.tree.get_node_context(node).map_or([0.0; 2], |e| e.translate);
        let at = (parent.0 + layout.location.x + shift[0], parent.1 + layout.location.y + shift[1]);
        let [x0, y0] = self.snap(at.0, at.1);
        let [x1, y1] = self.snap(at.0 + layout.size.width, at.1 + layout.size.height);
        let inset = (layout.border.left + layout.padding.left, layout.border.top + layout.padding.top);
        let origin = self.snap(at.0 + inset.0, at.1 + inset.1);
        let inner = layout.size.width - inset.0 - layout.border.right - layout.padding.right;
        let inner_height = layout.size.height - inset.1 - layout.border.bottom - layout.padding.bottom;
        let outside = x1 <= clip[0] || y1 <= clip[1] || x0 >= clip[2] || y0 >= clip[3];
        // Inside its border, where a box that does not let its content show past it cuts it off.
        let [l, t] = self.snap(at.0 + layout.border.left, at.1 + layout.border.top);
        let [r, b] =
            self.snap(at.0 + layout.size.width - layout.border.right, at.1 + layout.size.height - layout.border.bottom);
        let inside = [clip[0].max(l), clip[1].max(t), clip[2].min(r), clip[3].min(b)];
        let Some(element) = self.tree.get_node_context_mut(node) else { return };
        let mut look = Look::of(&element.computed);
        let mut text_color = element.text_style.color.linear_premultiplied();
        let mut own_opacity = element.computed.opacity;
        if !element.animations.is_empty() || element.keyframes.is_some() {
            look.background = animation::showing(element, Animated::Background, self.time);
            look.border_color = animation::showing(element, Animated::BorderColor, self.time);
            text_color = animation::showing(element, Animated::Color, self.time);
            own_opacity = animation::showing(element, Animated::Opacity, self.time)[0];
            animation::settle(element, self.time);
        }
        let opacity = opacity * own_opacity;
        let (overflow, scroll, scroll_max, bar) =
            (element.computed.overflow, element.scroll, element.scroll_max, element.computed.scrollbar);
        // Its own text is its content as much as its children are, so a label that
        // hides its overflow keeps a long line within it, as a table's cell does.
        let text_clip = if overflow == Overflow::Visible { clip } else { inside };
        if element.see_through && !outside {
            cut_out(self.quads, inside);
        }
        if !outside {
            let rect = [x0, y0, x1 - x0, y1 - y0];
            if let Some(shadow) = &element.computed.shadow {
                shadow_quad(shadow, &look, rect, self.scale, opacity, clip, self.quads);
            }
            let shaded = element.computed.shader.as_deref().and_then(|name| match self.shaders.get(name) {
                Some(shader) => {
                    let first = self.values.len() as u32;
                    self.values.extend(shader.values(&element.vars));
                    Some((shader.number, first))
                }
                None => {
                    self.unknown_shaders.push(name.to_string());
                    None
                }
            });
            shape_quad(&look, rect, (self.scale, opacity, shaded), clip, self.quads);
            side_quads(&look, rect, self.scale, opacity, clip, self.quads);
            if let Some(gradient) = element.gradient
                && opacity > 0.0
            {
                let straight = |c: crate::Color| [c.r, c.g, c.b].map(|v| f32::from(v) / 255.0);
                let with_alpha = |c: crate::Color| {
                    let [r, g, b] = straight(c);
                    [r, g, b, f32::from(c.a) / 255.0 * opacity]
                };
                self.quads.push(Quad {
                    rect: [x0, y0, x1 - x0, y1 - y0],
                    uv: gradient.bottom.map_or([0.0; 4], with_alpha),
                    color: with_alpha(gradient.left),
                    border_color: with_alpha(gradient.right),
                    clip,
                    radius: look.radius * self.scale,
                    kind: Quad::GRADIENT,
                    flags: if gradient.bottom.is_some() { Quad::FADES_DOWN } else { 0 },
                    ..Quad::default()
                });
            }
            let content = [origin[0], origin[1], inner * self.scale, inner_height * self.scale];
            shape_quads(&element.shapes, content, self.scale, opacity, clip, (self.atlas, self.quads));
            if let Some(icon) = element.icon {
                let color = text_color.map(|c| c * opacity);
                icon_quad(icon, content, self.scale, color, clip, (self.atlas, self.quads));
            }
            if let Some(image) = element.image
                && opacity > 0.0
            {
                let rect = [x0, y0, x1 - x0, y1 - y0];
                self.quads.push(Quad {
                    rect,
                    uv: image.uv,
                    color: [opacity; 4],
                    clip,
                    radius: look.radius * self.scale,
                    kind: Quad::IMAGE,
                    flags: image.texture,
                    ..Quad::default()
                });
            }
            if let Some(Control::TextField(_)) = &element.control {
                let content =
                    [origin[0], origin[1], origin[0] + inner * self.scale, origin[1] + inner_height * self.scale];
                // Its thumb runs in its padding, outside its text's clip.
                let outer = clip;
                let clip = [
                    clip[0].max(content[0]),
                    clip[1].max(content[1]),
                    clip[2].min(content[2]),
                    clip[3].min(content[3]),
                ];
                let focused = self.focused == Some(node);
                let size = [inner * self.scale, inner_height * self.scale];
                // Its thumb runs inside its border, as a scroll view's does.
                let edge = x1 - look.border * self.scale;
                let bar = (edge, outer);
                let field =
                    Field { focused, caret: focused && self.caret_on, opacity, clip, size, bar, scale: self.scale };
                draw_field(element, origin, field, self.atlas, self.quads);
            }
            let color = text_color.map(|c| c * opacity);
            if let Some(text) = &mut element.text
                && let Some((_, shaped)) = &mut text.shaped
            {
                // Broken again at the width it ended up, which the last
                // measure may not have been.
                let width = element.text_style.wrap.then_some((inner * self.scale).ceil());
                shaped.break_all_lines(width);
                // Right to left, the start is the right, whichever way the text itself runs.
                let rtl = element.text_style.rtl;
                let alignment = match (element.text_style.align, rtl) {
                    (TextAlign::Start, false) | (TextAlign::End, true) => Alignment::Left,
                    (TextAlign::Start, true) | (TextAlign::End, false) => Alignment::Right,
                    (TextAlign::Center, _) => Alignment::Center,
                };
                shaped.align(alignment, AlignmentOptions::default());
                // Text that does not wrap is laid out at its own width, so its
                // alignment has nothing to align it in: it is placed in its box
                // here, and starts at the start when it does not fit.
                let room = if width.is_none() { (inner * self.scale - shaped.width()).max(0.0) } else { 0.0 };
                let across = match (element.text_style.align, rtl) {
                    (TextAlign::Start, false) | (TextAlign::End, true) => 0.0,
                    (TextAlign::Center, _) => (room / 2.0).round(),
                    _ => room.round(),
                };
                // Centred down a box taller than the text, such as a button
                // with a set height, as Unity's middle alignment does.
                let spare = (inner_height * self.scale - shaped.height()).max(0.0);
                let origin = [origin[0] + across, origin[1] + (spare / 2.0).round()];
                let paint = Paint { color, spans: &text.spans, style: &element.text_style, opacity, scale: self.scale };
                glyph_quads(shaped, origin, &paint, text_clip, self.atlas, self.quads);
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
            shape_quad(&look, [l, t, r - l, b - t], (self.scale, opacity, None), child_clip, self.quads);
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
    /// The widths of a border on some sides only, left, top, right and
    /// bottom, drawn as strips rather than with the shape.
    sides: Option<[f32; 4]>,
    dashed: bool,
    radius: f32,
    /// What is behind shows through, blurred.
    backdrop: bool,
}

impl Look {
    fn of(style: &Style) -> Self {
        let (border, border_color) = style.border.map_or((0.0, [0.0; 4]), |(w, c)| (w, c.linear_premultiplied()));
        Self {
            background: style.background.map_or([0.0; 4], |c| c.linear_premultiplied()),
            border: if style.border_sides.is_some() { 0.0 } else { border },
            border_color,
            sides: style.border_sides,
            dashed: style.dashed,
            radius: style.radius,
            backdrop: style.backdrop_blur > 0.0,
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
/// Takes away what is drawn so far inside `hole` (left, top, right and
/// bottom): a quad partly in it is drawn as the parts around it, each
/// clipped to its own band.
fn cut_out(quads: &mut Vec<Quad>, hole: [f32; 4]) {
    if hole[0] >= hole[2] || hole[1] >= hole[3] {
        return;
    }
    for quad in std::mem::take(quads) {
        let ([x, y, w, h], clip) = (quad.rect, quad.clip);
        let shown = [clip[0].max(x), clip[1].max(y), clip[2].min(x + w), clip[3].min(y + h)];
        let apart = shown[2] <= hole[0] || shown[0] >= hole[2] || shown[3] <= hole[1] || shown[1] >= hole[3];
        if apart {
            quads.push(quad);
            continue;
        }
        let (top, bottom) = (hole[1].max(shown[1]), hole[3].min(shown[3]));
        let around = [
            [shown[0], shown[1], shown[2], hole[1]],
            [shown[0], hole[3], shown[2], shown[3]],
            [shown[0], top, hole[0], bottom],
            [hole[2], top, shown[2], bottom],
        ];
        let parts = around.into_iter().filter(|part| part[0] < part[2] && part[1] < part[3]);
        quads.extend(parts.map(|clip| Quad { clip, ..quad }));
    }
}

fn shape_quads(
    shapes: &[Shape],
    content: [f32; 4],
    scale: f32,
    opacity: f32,
    clip: [f32; 4],
    (atlas, quads): (&mut GlyphAtlas, &mut Vec<Quad>),
) {
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
                    sides: None,
                    dashed: false,
                    radius,
                    backdrop: false,
                };
                shape_quad(&look, [c[0] - r, c[1] - r, r * 2.0, r * 2.0], (scale, opacity, None), clip, quads);
            }
            Shape::Rect { rect: [x, y, w, h], radius, fill } => {
                let corner = at([x, y]);
                let look = Look {
                    background: fill.linear_premultiplied(),
                    border: 0.0,
                    border_color: [0.0; 4],
                    sides: None,
                    dashed: false,
                    backdrop: false,
                    radius,
                };
                shape_quad(&look, [corner[0], corner[1], w * scale, h * scale], (scale, opacity, None), clip, quads);
            }
            Shape::Icon { name, center, size, color } => {
                let (c, size) = (at(center), size * scale);
                let color = color.linear_premultiplied().map(|c| c * opacity);
                icon_quad(name, [c[0] - size / 2.0, c[1] - size / 2.0, size, size], scale, color, clip, (atlas, quads));
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

/// A box's background and border. One drawn by a shader, numbered in
/// `shaded` with where its values start, is drawn even when its background
/// is clear, which the shader may fill; its colour is left unfaded, and its
/// opacity and the display scale go to the shader beside it.
fn shape_quad(
    look: &Look,
    rect: [f32; 4],
    (scale, opacity, shaded): (f32, f32, Option<(u32, u32)>),
    clip: [f32; 4],
    quads: &mut Vec<Quad>,
) {
    let border = look.border > 0.0 && look.border_color[3] > 0.0;
    if (look.background[3] <= 0.0 && !border && !look.backdrop && shaded.is_none()) || opacity <= 0.0 {
        return;
    }
    let border = if border { (look.border * scale).round().max(1.0) } else { 0.0 };
    let flags = if look.dashed { Quad::DASHED } else { 0 } | if look.backdrop { Quad::BACKDROP } else { 0 };
    let (color, uv, flags) = match shaded {
        Some((number, first)) => (
            look.background,
            [f32::from_bits(first), scale, opacity, 0.0],
            flags | Quad::SHADED | (number & 0xffff) << Quad::SHADER_SHIFT,
        ),
        None => (look.background.map(|c| c * opacity), [0.0; 4], flags),
    };
    quads.push(Quad {
        rect,
        uv,
        color,
        border_color: look.border_color.map(|c| c * opacity),
        clip,
        radius: look.radius * scale,
        border,
        kind: Quad::SHAPE,
        flags,
    });
}

/// A border on some sides only: each a strip along the inside of the box,
/// as wide as its side, in the border's colour.
fn side_quads(look: &Look, rect: [f32; 4], scale: f32, opacity: f32, clip: [f32; 4], quads: &mut Vec<Quad>) {
    let Some(sides) = look.sides else { return };
    let color = look.border_color.map(|c| c * opacity);
    if color[3] <= 0.0 {
        return;
    }
    let [x, y, width, height] = rect;
    let [left, top, right, bottom] = sides.map(|side| if side > 0.0 { (side * scale).round().max(1.0) } else { 0.0 });
    let strips = [
        [x, y, left, height],
        [x, y, width, top],
        [x + width - right, y, right, height],
        [x, y + height - bottom, width, bottom],
    ];
    for strip in strips.into_iter().filter(|strip| strip[2] > 0.0 && strip[3] > 0.0) {
        quads.push(Quad { rect: strip, color, clip, kind: Quad::SHAPE, ..Quad::default() });
    }
}

/// How a text's glyphs are coloured: all one colour, or for rich text, as
/// each span asks, with the text style's colours for links, code and marks.
struct Paint<'a> {
    color: [f32; 4],
    spans: &'a [crate::Span],
    style: &'a crate::TextStyle,
    opacity: f32,
    scale: f32,
}

impl Paint<'_> {
    /// One colour for everything, as a text field is drawn.
    fn plain(color: [f32; 4]) -> Paint<'static> {
        static STYLE: std::sync::LazyLock<crate::TextStyle> = std::sync::LazyLock::new(crate::TextStyle::default);
        Paint { color, spans: &[], style: &STYLE, opacity: 1.0, scale: 1.0 }
    }

    fn fade(&self, color: Color) -> [f32; 4] {
        color.linear_premultiplied().map(|c| c * self.opacity)
    }

    /// A span's text colour.
    fn color(&self, span: Option<&crate::Span>) -> [f32; 4] {
        match span {
            Some(crate::Span { color: Some(color), .. }) => self.fade(*color),
            Some(span) if span.marks.broken => self.color.map(|c| c * 0.6),
            Some(span) if span.link.is_some() => self.fade(self.style.link_color),
            _ => self.color,
        }
    }
}

/// Every glyph of a shaped text, on whole pixels: hinted glyphs are drawn
/// where the hinting expected them. Rich text's grounds go under its
/// glyphs, and its lines through or under them.
fn glyph_quads(
    layout: &Layout<u16>,
    origin: [f32; 2],
    paint: &Paint,
    clip: [f32; 4],
    atlas: &mut GlyphAtlas,
    quads: &mut Vec<Quad>,
) {
    if paint.color[3] <= 0.0 {
        return;
    }
    let size = atlas.size() as f32;
    for line in layout.lines() {
        for item in line.items() {
            let PositionedLayoutItem::GlyphRun(glyph_run) = item else { continue };
            let run = glyph_run.run();
            let brush = glyph_run.style().brush as usize;
            let span = brush.checked_sub(1).and_then(|index| paint.spans.get(index));
            let color = paint.color(span);
            let metrics = *run.font_metrics();
            let left = origin[0] + glyph_run.offset();
            let baseline = origin[1] + glyph_run.baseline();
            let width = glyph_run.advance();
            if let Some(span) = span
                && (span.marks.code || span.marks.mark)
            {
                let (ground, pad) = if span.marks.code {
                    (paint.style.code_background, 2.0 * paint.scale)
                } else {
                    (paint.style.mark_color, 0.0)
                };
                let top = (baseline - metrics.ascent).round();
                let rect = [
                    (left - pad).round(),
                    top,
                    (width + pad * 2.0).round(),
                    (baseline + metrics.descent).round() - top,
                ];
                let look = Look { background: ground.linear_premultiplied(), radius: 3.0, ..Look::default() };
                shape_quad(&look, rect, (paint.scale, paint.opacity, None), clip, quads);
            }
            let font = run.font();
            let font = (font.data.data(), font.index, font.data.id());
            let coords: Vec<i16> = run.normalized_coords().iter().map(|c| c.to_bits()).collect();
            let synthesis = run.synthesis();
            let synthesis = crate::atlas::Synthesis { skew: synthesis.skew(), embolden: synthesis.embolden() };
            for glyph in glyph_run.positioned_glyphs() {
                let found = atlas.glyph(font, glyph.id as u16, run.font_size(), &coords, synthesis);
                let Some(found) = found else { continue };
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
            let style = glyph_run.style();
            let lines = [
                style.strikethrough.as_ref().map(|d| (d, metrics.strikethrough_offset, metrics.strikethrough_size)),
                style.underline.as_ref().map(|d| (d, metrics.underline_offset, metrics.underline_size)),
            ];
            for (decoration, offset, thickness) in lines.into_iter().flatten() {
                let offset = decoration.offset.unwrap_or(offset);
                let thickness = decoration.size.unwrap_or(thickness).round().max(1.0);
                let rect = [left.round(), (baseline - offset).round(), width.round(), thickness];
                quads.push(Quad { rect, color, clip, kind: Quad::SHAPE, ..Quad::default() });
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
    /// Its content box's size, in physical pixels, and where its thumb can
    /// show: its right edge inside its border, and what clips the field.
    size: [f32; 2],
    bar: (f32, [f32; 4]),
    scale: f32,
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
            glyph_quads(layout, at, &Paint::plain(fade(edit.placeholder.style.color)), field.clip, atlas, quads);
        }
    } else if let Some(layout) = edit.editor.try_layout() {
        glyph_quads(layout, at, &Paint::plain(fade(element.text_style.color)), field.clip, atlas, quads);
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
    // Text taller than the field has a thumb, as a scroll view does.
    if edit.multiline
        && let Some(layout) = edit.editor.try_layout()
        && let Some((top, length)) = thumb(field.size[1], layout.height() - field.size[1], edit.scroll_y)
    {
        let (width, inset) = (BAR * field.scale, BAR_INSET * field.scale);
        let (edge, clip) = field.bar;
        let rect = [(edge - width - inset).round(), (origin[1] + top).round(), width, length];
        let look = Look { background: style.scrollbar.linear_premultiplied(), radius: BAR / 2.0, ..Look::default() };
        shape_quad(&look, rect, (field.scale, field.opacity, None), clip, quads);
    }
}
