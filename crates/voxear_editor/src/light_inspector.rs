//! What the Inspector shows of a Light, laid out as a tool of its own
//! rather than a plain list: its type as a row of icon buttons, a
//! picture of the light, then its rows under headings, Light, Shape,
//! Shadows and Baking, with sliders where a value has a sensible span and
//! units after the numbers.
//!
//! The picture follows the light as it changes. A directional light shows
//! the sky it lights, its colour and strength, and where it stands: its
//! disc on the sky as high as the light is above the horizon, at its heading
//! along it, with the height and heading written beside it. A local light
//! shows its glow, as wide as it reaches, and a spot light its cone.

use voxear_core::math::Vec3;
use voxear_ecs::{Entity, World};
use voxear_engine::script_types::FieldInspector;
use voxear_render::{Light, LightKind, temperature_color};
use voxear_scene::{Transform, WorldTransform};
use voxear_ui::{Color, NodeId, Shape, Style, Ui};

use crate::props::Prop;

/// The icons of the types' buttons, in the order the types are offered.
const KIND_ICONS: [&str; 6] = ["light", "bulb", "spot-light", "rect-light", "disc-light", "tube-light"];
/// How tall the picture is.
const HEIGHT: f32 = 92.0;
/// Bands the sky's gradient is drawn in.
const SKY_BANDS: usize = 48;

/// The rows of a Light, in the groups its headings name, as its type makes
/// them matter.
pub fn props(light: &Light) -> Vec<Prop> {
    let kind = light.kind;
    let local = kind != LightKind::Directional;
    let mut props = vec![Prop::LightKind, Prop::LightAppearance, Prop::LightColor];
    if light.temperature.is_some() {
        props.push(Prop::LightTemperature);
    }
    props.push(Prop::Intensity);
    if local {
        props.push(Prop::Range);
    }
    // Shape.
    match kind {
        LightKind::Directional => props.extend([Prop::LightBody, Prop::LightCookie, Prop::LightCookieSize]),
        LightKind::Point => props.extend([Prop::LightRadius, Prop::LightProfile]),
        LightKind::Spot => {
            props.extend([Prop::SpotInner, Prop::SpotOuter, Prop::LightRadius, Prop::LightCookie, Prop::LightProfile])
        }
        LightKind::Rectangle => props.extend([Prop::LightWidth, Prop::LightHeight]),
        LightKind::Disc => props.push(Prop::LightRadius),
        LightKind::Tube => props.extend([Prop::LightLength, Prop::LightRadius]),
    }
    // Shadows.
    match kind {
        LightKind::Directional => props.extend([Prop::LightSoftness, Prop::LightShadowTint]),
        _ => props.extend([Prop::LightShadows, Prop::LightSoftShadows]),
    }
    props.push(Prop::LightMode);
    props
}

/// The heading a row starts, when it is the first of its group.
fn heading(light: &Light, prop: &Prop) -> Option<&'static str> {
    let group = |prop: &Prop| match prop {
        Prop::LightKind => None,
        Prop::LightAppearance | Prop::LightColor | Prop::LightTemperature | Prop::Intensity | Prop::Range => {
            Some("Light")
        }
        Prop::LightSoftness | Prop::LightShadowTint | Prop::LightShadows | Prop::LightSoftShadows => Some("Shadows"),
        Prop::LightMode => Some("Baking"),
        _ => Some("Shape"),
    };
    let rows = props(light);
    let first = rows.iter().find(|row| group(row) == group(prop))?;
    (first == prop).then(|| group(prop)).flatten()
}

/// How a Light's row shows: its type and other short choices as buttons, a
/// heading over the first row of each group, a slider for an angle or a
/// temperature, and its unit.
pub fn adjust(meta: &mut FieldInspector, light: &Light, prop: &Prop) {
    if let Some(title) = heading(light, prop) {
        meta.title = Some(title.to_string());
    }
    match prop {
        Prop::LightKind => {
            meta.enum_buttons = true;
            meta.hide_label = true;
            meta.option_icons = KIND_ICONS.iter().map(|icon| icon.to_string()).collect();
        }
        Prop::LightMode | Prop::LightBody => meta.enum_buttons = true,
        Prop::LightTemperature => {
            meta.range = Some([1667.0, 20000.0]);
            meta.suffix = Some("K".into());
        }
        Prop::SpotInner | Prop::SpotOuter => {
            meta.range = Some([1.0, 179.0]);
            meta.suffix = Some("°".into());
        }
        Prop::LightSoftness => {
            meta.min = Some(0.0);
            meta.suffix = Some("°".into());
        }
        Prop::Range | Prop::LightRadius | Prop::LightWidth | Prop::LightHeight | Prop::LightLength => {
            meta.min = Some(0.0);
            meta.suffix = Some("m".into());
        }
        Prop::LightCookieSize => meta.suffix = Some("m".into()),
        Prop::Intensity => meta.min = Some(0.0),
        _ => {}
    }
}

/// The picture of a Light at the top of its section.
pub struct LightCard {
    card: NodeId,
    picture: NodeId,
    title: NodeId,
    detail: NodeId,
    /// What it last drew, to draw again only when that changes.
    drawn: Option<(Light, [f32; 3], [f32; 4])>,
}

impl LightCard {
    pub fn new(ui: &mut Ui, parent: NodeId) -> Self {
        let card = ui.panel(parent, Style::column());
        ui.add_class(card, "light-card");
        ui.set_inline_property(card, "height", &format!("{HEIGHT}px"));
        let picture = ui.panel(card, Style::default());
        ui.add_class(picture, "light-card-picture");
        let title = ui.add_label(card, "");
        ui.add_class(title, "light-card-title");
        let detail = ui.add_label(card, "");
        ui.add_class(detail, "light-card-detail");
        Self { card, picture, title, detail, drawn: None }
    }

    /// Draws the light as it is now, when it changed.
    pub fn update(&mut self, ui: &mut Ui, world: &World, entity: Entity) {
        let Some(light) = world.get::<Light>(entity).copied() else { return };
        let (elevation, heading) = aim(world, entity);
        let rect = ui.rect(self.picture);
        let now = (light, [elevation, heading, 0.0], rect);
        if self.drawn.as_ref() == Some(&now) || rect[2] <= 0.0 {
            return;
        }
        self.drawn = Some(now);
        let (width, height) = (rect[2], rect[3]);
        let colour = shown_colour(&light);
        let shapes = match light.kind {
            LightKind::Directional => sky(width, height, colour, elevation, heading),
            _ => glow(width, height, colour, &light),
        };
        ui.set_shapes(self.picture, shapes);
        let temperature = light.temperature.map(|k| format!("{k:.0} K · ")).unwrap_or_default();
        let (title, detail) = match light.kind {
            LightKind::Directional => {
                let body = if light.body == voxear_render::SkyBody::Moon { "Moon" } else { "Sun" };
                let where_ = if elevation < 0.0 { "below the horizon" } else { "up" };
                (
                    format!("{body} · {:.0}° {where_}", elevation.abs()),
                    format!("{temperature}intensity {} · heading {heading:.0}°", strength(light.intensity)),
                )
            }
            kind => (
                format!("{} · {:.1} m", kind_name(kind), light.range),
                format!("{temperature}intensity {}", strength(light.intensity)),
            ),
        };
        ui.set_text(self.title, &title);
        ui.set_text(self.detail, &detail);
        let _ = self.card;
    }
}

/// How high a light points down from above the horizon, and its heading
/// round from north (+Z) towards east (+X), both in degrees: where in the sky
/// a directional light shines from.
fn aim(world: &World, entity: Entity) -> (f32, f32) {
    let rotation = match world.get::<WorldTransform>(entity) {
        Some(placed) => placed.0.to_scale_rotation_translation().1,
        None => world.get::<Transform>(entity).map_or_else(Default::default, |t| t.rotation),
    };
    // A light shines along its forward; the sky it comes from is the other way.
    let from = -(rotation * Vec3::Z);
    let elevation = from.y.clamp(-1.0, 1.0).asin().to_degrees();
    let heading = from.x.atan2(from.z).to_degrees().rem_euclid(360.0);
    (elevation, heading)
}

/// The light's colour as it shines, its filter over its temperature's, in
/// sRGB from 0 to 1, brightest channel 1.
fn shown_colour(light: &Light) -> [f32; 3] {
    let linear = match light.temperature {
        Some(kelvin) => light.color * temperature_color(kelvin),
        None => light.color,
    };
    let top = linear.max_element().max(1e-4);
    let srgb = |c: f32| {
        let c = (c / top).clamp(0.0, 1.0);
        if c <= 0.003_130_8 { c * 12.92 } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 }
    };
    [srgb(linear.x), srgb(linear.y), srgb(linear.z)]
}

/// A number of a light's strength, short: 59 576 as 59.6k.
fn strength(intensity: f32) -> String {
    match intensity {
        i if i >= 10_000.0 => format!("{:.1}k", i / 1000.0),
        i if i >= 100.0 => format!("{i:.0}"),
        i => format!("{i:.2}"),
    }
}

fn kind_name(kind: LightKind) -> &'static str {
    match kind {
        LightKind::Directional => "Directional",
        LightKind::Point => "Point",
        LightKind::Spot => "Spot",
        LightKind::Rectangle => "Rectangle",
        LightKind::Disc => "Disc",
        LightKind::Tube => "Tube",
    }
}

fn rgba([r, g, b]: [f32; 3], alpha: f32) -> Color {
    let byte = |c: f32| (c.clamp(0.0, 1.0) * 255.0).round() as u8;
    Color::rgba(byte(r), byte(g), byte(b), byte(alpha))
}

fn mix(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

/// A soft disc: rings fading out from `radius` to nothing at `reach`.
fn halo(centre: [f32; 2], radius: f32, reach: f32, colour: [f32; 3], strength: f32) -> Vec<Shape> {
    const RINGS: usize = 32;
    (0..RINGS)
        .map(|ring| {
            let t = 1.0 - ring as f32 / RINGS as f32;
            Shape::Circle {
                center: centre,
                radius: radius + (reach - radius) * t,
                fill: rgba(colour, strength * (1.0 - t).powi(2) * 0.12),
                border: None,
            }
        })
        .collect()
}

/// The sky a directional light lights: darker overhead, lighter and tinted
/// by the light at the horizon, a line for the horizon with a tick at each
/// quarter of the compass, and its disc where it stands.
fn sky(width: f32, height: f32, colour: [f32; 3], elevation: f32, heading: f32) -> Vec<Shape> {
    let horizon = height * 0.78;
    // Night when it is under the horizon, the deeper the darker.
    let day = ((elevation + 6.0) / 20.0).clamp(0.0, 1.0);
    let top = mix([0.05, 0.06, 0.10], [0.16, 0.30, 0.55], day);
    let low = mix(mix([0.10, 0.10, 0.16], [0.55, 0.68, 0.85], day), colour, 0.25 * day);
    let mut shapes = Vec::new();
    let band = horizon / SKY_BANDS as f32;
    for at in 0..SKY_BANDS {
        let t = at as f32 / (SKY_BANDS - 1) as f32;
        shapes.push(Shape::Rect {
            rect: [0.0, at as f32 * band, width, band + 0.5],
            radius: 0.0,
            fill: rgba(mix(top, low, t), 1.0),
        });
    }
    shapes.push(Shape::Rect {
        rect: [0.0, horizon, width, height - horizon],
        radius: 0.0,
        fill: rgba([0.09, 0.10, 0.11], 1.0),
    });
    let line = rgba([1.0, 1.0, 1.0], 0.35);
    shapes.push(Shape::Line { from: [0.0, horizon], to: [width, horizon], width: 1.0, color: line });
    for quarter in 0..=4 {
        let x = width * quarter as f32 / 4.0;
        shapes.push(Shape::Line { from: [x, horizon], to: [x, horizon + 4.0], width: 1.0, color: line });
    }
    // The path it would take at this height, a curve over the sky.
    let peak = horizon - 8.0 - (horizon - 16.0) * (elevation.max(0.0) / 90.0);
    let mut last = [0.0, horizon];
    for step in 1..=32 {
        let t = step as f32 / 32.0;
        let point = [width * t, horizon - (horizon - peak) * (std::f32::consts::PI * t).sin()];
        shapes.push(Shape::Line { from: last, to: point, width: 1.0, color: rgba([1.0, 1.0, 1.0], 0.22) });
        last = point;
    }
    let x = width * heading / 360.0;
    let y = horizon - (horizon - 10.0) * (elevation / 90.0).clamp(-0.15, 1.0);
    shapes.extend(halo([x, y], 6.0, 26.0, colour, 1.0));
    shapes.push(Shape::Circle {
        center: [x, y],
        radius: 6.0,
        fill: rgba(colour, 1.0),
        border: Some((1.0, rgba([1.0, 1.0, 1.0], 0.8))),
    });
    shapes
}

/// A local light's glow on a dark ground: its colour fading out as far as
/// it reaches, the light at its middle, and a spot light's cone.
fn glow(width: f32, height: f32, colour: [f32; 3], light: &Light) -> Vec<Shape> {
    let mut shapes =
        vec![Shape::Rect { rect: [0.0, 0.0, width, height], radius: 0.0, fill: rgba([0.07, 0.075, 0.085], 1.0) }];
    let centre = [width * 0.72, height * 0.42];
    // Wider for a light that reaches farther, within the picture.
    let reach = (height * 0.35 + light.range.max(0.0).sqrt() * 6.0).min(width * 0.45);
    if light.kind == LightKind::Spot {
        let half = |degrees: f32| (degrees.clamp(1.0, 179.0) / 2.0).to_radians();
        let cone = |angle: f32, alpha: f32, shapes: &mut Vec<Shape>| {
            let length = height * 0.9;
            for side in [-1.0, 1.0] {
                let end = [centre[0] + side * angle.sin() * length, centre[1] + angle.cos() * length];
                shapes.push(Shape::Line { from: centre, to: end, width: 1.0, color: rgba(colour, alpha) });
            }
        };
        shapes.extend(halo([centre[0], centre[1] + reach * 0.4], 2.0, reach, colour, 0.8));
        cone(half(light.spot_outer_degrees), 0.55, &mut shapes);
        cone(half(light.spot_inner_degrees), 0.9, &mut shapes);
    } else {
        shapes.extend(halo(centre, 4.0, reach, colour, 1.0));
    }
    let size = match light.kind {
        LightKind::Rectangle => {
            let (w, h) = (light.width.max(0.1), light.height.max(0.1));
            let scale = 18.0 / w.max(h);
            shapes.push(Shape::Rect {
                rect: [centre[0] - w * scale / 2.0, centre[1] - h * scale / 2.0, w * scale, h * scale],
                radius: 1.5,
                fill: rgba(colour, 1.0),
            });
            0.0
        }
        LightKind::Tube => {
            let half = 12.0;
            shapes.push(Shape::Line {
                from: [centre[0] - half, centre[1]],
                to: [centre[0] + half, centre[1]],
                width: 4.0,
                color: rgba(colour, 1.0),
            });
            0.0
        }
        _ => 5.0,
    };
    if size > 0.0 {
        shapes.push(Shape::Circle {
            center: centre,
            radius: size,
            fill: rgba(colour, 1.0),
            border: Some((1.0, rgba([1.0, 1.0, 1.0], 0.8))),
        });
    }
    shapes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_come_in_their_groups_each_headed_once() {
        let mut light = Light::spot(Vec3::ONE, 1.0, 10.0, 20.0, 40.0);
        light.temperature = Some(4000.0);
        let rows = props(&light);
        let headings: Vec<(&Prop, &str)> = rows.iter().filter_map(|row| Some((row, heading(&light, row)?))).collect();
        let names: Vec<&str> = headings.iter().map(|(_, h)| *h).collect();
        assert_eq!(names, ["Light", "Shape", "Shadows", "Baking"]);
        assert_eq!(headings[0].0, &Prop::LightAppearance);
        assert_eq!(headings[1].0, &Prop::SpotInner);
        let sun = Light::directional(Vec3::ONE, 1.0);
        assert!(!props(&sun).contains(&Prop::Range), "the sun reaches everywhere");
    }

    #[test]
    fn the_colour_shown_is_the_filter_over_its_temperature() {
        let warm = Light { temperature: Some(1900.0), ..Light::point(Vec3::ONE, 1.0, 10.0) };
        let [r, g, b] = shown_colour(&warm);
        assert!(r > g && g > b, "a candle is orange: {r} {g} {b}");
        assert!((r - 1.0).abs() < 1e-4, "brightest channel full: {r}");
    }
}
