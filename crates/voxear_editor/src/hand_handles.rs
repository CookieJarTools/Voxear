//! The Gun Editor's handles in its view: a ring on each hand, and a cross
//! on the gun. Dragging a hand slides it over the gun: the line from the
//! eye through the pointer meets the gun's surfaces, and the hand holds
//! there, its palm put on the surface and its fingers closed round what is
//! under them by the game's own hands, so the view shows just what the
//! game will. With Alt held it moves freely instead, in the plane facing
//! the view, as to a pouch. Dragging the cross moves where the gun is held.
//! What a drag changes is the editor's to say: see `editor::gun_handlings`.

use voxear_core::math::{Mat4, Vec3};
use voxear_engine::guns::{
    actions::Hand,
    contact::{self, Shape},
};
use voxear_ui::{Color, NodeId, Shape as Drawn, Ui};

use crate::scene_tools::ViewCamera;

/// How near a handle, in pixels, a press takes it.
const PICK: f32 = 14.0;
const RING: f32 = 9.0;
const RIGHT: u32 = 0x6f9bff;
const LEFT: u32 = 0x58c49a;
const GUN: u32 = 0xffb44a;

/// What a handle moves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Grab {
    Hand(Hand),
    Gun,
}

/// What the handles are drawn from this frame: the view's camera, where
/// the gun is and its surfaces in its own space, each hand's palm, left
/// then right, and the eyes it is held before, all in the world.
pub(crate) struct Seen {
    pub camera: ViewCamera,
    pub gun: Mat4,
    pub shapes: Vec<Shape>,
    pub palms: [Option<Vec3>; 2],
    pub eye: Mat4,
}

/// A drag under way: what it holds, the value it changes as it began,
/// and where the pointer's point began, in the gun's space for a hand and
/// the world for the gun, with the plane a free move keeps to.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Held {
    grab: Grab,
    from: [f32; 3],
    start: Vec3,
    plane: (Vec3, Vec3),
    /// The way out of the gun's face a hand started on, in the gun's space.
    face: Option<Vec3>,
}

/// What a drag asks: `grab`'s value set to this, or the drag over.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum HandleEdit {
    Moved(Grab, [f32; 3]),
    Done,
}

#[derive(Default)]
pub(crate) struct HandHandles {
    held: Option<Held>,
    was_pressed: bool,
}

impl HandHandles {
    /// Whether a drag is under way, which the Orbit view's turning leaves
    /// alone.
    pub fn is_held(&self) -> bool {
        self.held.is_some()
    }

    /// Draws the handles on `layer`, and drags them by presses on
    /// `viewport`. `value` gives what a grab's drag changes as it starts.
    pub fn update(
        &mut self,
        ui: &mut Ui,
        (viewport, layer): (NodeId, NodeId),
        seen: &Seen,
        free: bool,
        value: impl Fn(Grab) -> Option<[f32; 3]>,
    ) -> Option<HandleEdit> {
        let pressed = ui.is_pressed(viewport);
        let began = pressed && !self.was_pressed;
        self.was_pressed = pressed;
        let pointer = ui.pointer_position();
        let handles = handles(seen);
        if began && let Some((x, y)) = pointer {
            self.held = self.grab(seen, &handles, [x, y], &value);
        }
        self.draw(ui, layer, seen, &handles);
        let held = self.held?;
        if !pressed {
            self.held = None;
            return Some(HandleEdit::Done);
        }
        let (x, y) = pointer?;
        let (origin, way) = seen.camera.ray([x, y]);
        let moved = match held.grab {
            Grab::Hand(_) => {
                let into = seen.gun.inverse();
                let (from, along) = (into.transform_point3(origin), into.transform_vector3(way));
                let on = if free { None } else { contact::hit(&seen.shapes, from, along) };
                let point = match (on, held.face) {
                    // On a face that way too: there, as the hand slides over the gun.
                    (Some((point, facing)), Some(face)) if facing.dot(face) > 0.7 => point,
                    // On another face: along the one it started on, and across the view only, so it does not
                    // jump to another side of the gun.
                    (Some((point, _)), Some(face)) => {
                        let moved = point - held.start;
                        let moved = moved - face * moved.dot(face);
                        let deep = into.transform_vector3(seen.camera.forward);
                        let deep = (deep - face * deep.dot(face)).normalize_or_zero();
                        held.start + moved - deep * moved.dot(deep)
                    }
                    (Some((point, _)), None) => point,
                    (None, _) => into.transform_point3(meet(held.plane, origin, way)?),
                };
                Vec3::from(held.from) + point - held.start
            }
            Grab::Gun => {
                let point = meet(held.plane, origin, way)?;
                Vec3::from(held.from) + seen.eye.inverse().transform_vector3(point - held.start)
            }
        };
        Some(HandleEdit::Moved(held.grab, moved.to_array()))
    }

    /// What a press at `pointer` takes: the nearest handle near enough.
    fn grab(
        &self,
        seen: &Seen,
        handles: &[(Grab, Vec3)],
        pointer: [f32; 2],
        value: &impl Fn(Grab) -> Option<[f32; 3]>,
    ) -> Option<Held> {
        let near = |at: Vec3| seen.camera.project(at).map(|[x, y]| (x - pointer[0]).hypot(y - pointer[1]));
        let (grab, at) = handles
            .iter()
            .filter_map(|&(grab, at)| Some((grab, at, near(at)?)))
            .filter(|(.., distance)| *distance <= PICK)
            .min_by(|a, b| a.2.total_cmp(&b.2))
            .map(|(grab, at, _)| (grab, at))?;
        let plane = (at, seen.camera.forward);
        let start = match grab {
            Grab::Hand(_) => seen.gun.inverse().transform_point3(at),
            Grab::Gun => at,
        };
        let face = matches!(grab, Grab::Hand(_)).then(|| contact::facing(&seen.shapes, start)).flatten();
        Some(Held { grab, from: value(grab)?, start, plane, face })
    }

    fn draw(&self, ui: &mut Ui, layer: NodeId, seen: &Seen, handles: &[(Grab, Vec3)]) {
        let [left, top, ..] = ui.rect(layer);
        let mut shapes = Vec::new();
        for &(grab, at) in handles {
            let Some([x, y]) = seen.camera.project(at) else { continue };
            let center = [x - left, y - top];
            let lit = self.held.is_some_and(|held| held.grab == grab);
            let color = Color::hex(match grab {
                Grab::Hand(Hand::Right) => RIGHT,
                Grab::Hand(Hand::Left) => LEFT,
                Grab::Gun => GUN,
            });
            match grab {
                Grab::Hand(_) => {
                    let fill = color.with_alpha(if lit { 90 } else { 30 });
                    shapes.push(Drawn::Circle { center, radius: RING, fill, border: Some((2.0, color)) });
                }
                Grab::Gun => {
                    let reach = if lit { 9.0 } else { 7.0 };
                    let [cx, cy] = center;
                    shapes.push(Drawn::Line { from: [cx - reach, cy], to: [cx + reach, cy], width: 2.0, color });
                    shapes.push(Drawn::Line { from: [cx, cy - reach], to: [cx, cy + reach], width: 2.0, color });
                }
            }
        }
        ui.set_shapes(layer, shapes);
    }
}

/// The handles and where they are in the world: each hand's palm, and
/// the gun's middle.
fn handles(seen: &Seen) -> Vec<(Grab, Vec3)> {
    let hands = [Hand::Left, Hand::Right].into_iter().zip(seen.palms);
    let mut handles: Vec<(Grab, Vec3)> = hands.filter_map(|(hand, palm)| Some((Grab::Hand(hand), palm?))).collect();
    handles.push((Grab::Gun, seen.gun.w_axis.truncate()));
    handles
}

/// Where a line from `origin` along `way` meets a plane through a point
/// facing a way; none when it runs along it or away.
fn meet((point, facing): (Vec3, Vec3), origin: Vec3, way: Vec3) -> Option<Vec3> {
    let along = way.dot(facing);
    if along.abs() < 1e-6 {
        return None;
    }
    let t = (point - origin).dot(facing) / along;
    (t > 0.0).then(|| origin + way * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_meets_the_plane_facing_it_where_it_crosses() {
        let plane = (Vec3::new(0.0, 0.0, 2.0), Vec3::Z);
        let met = meet(plane, Vec3::ZERO, Vec3::new(0.5, 0.0, 1.0).normalize()).unwrap();
        assert!((met - Vec3::new(1.0, 0.0, 2.0)).length() < 1e-5, "{met}");
        assert_eq!(meet(plane, Vec3::ZERO, Vec3::X), None, "along it");
        assert_eq!(meet(plane, Vec3::ZERO, Vec3::NEG_Z), None, "away from it");
    }
}
