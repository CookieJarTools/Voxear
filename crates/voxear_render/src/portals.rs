//! Portals: an opening on an Obj that shows the place another Portal stands
//! in, and that Objs go through to it.
//!
//! The opening is a rectangle in its Obj's x and y, `size.x` wide about the
//! Obj and `size.y` high from it, facing along its +Z. What goes in at the
//! front of one comes out of the front of the other, so the far Portal is
//! the near one turned half round its up axis: [`Carry`] is that move, and
//! everything else here follows from it. Going through is the engine's work
//! (`voxear_engine::portals`); this is the component, the maths both share,
//! and what a view needs to draw the far side (`View::record`).

use voxear_core::math::{Mat4, Quat, Vec2, Vec3, Vec4};
use voxear_ecs::{Component, Entity, World};
use voxear_scene::{Destroying, Switched, Transform, WorldTransform};

/// An opening onto the place another Portal stands in.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Portal {
    /// The opening's width and height in metres: across the Obj's x, half
    /// to each side, and up its y from the Obj.
    pub size: Vec2,
    /// The Portal it leads to. With none it shows nothing and nothing goes
    /// through it, though others may lead to it.
    pub to: Option<Entity>,
    /// Whether it shows its far side and lets Objs through. Closed, it
    /// shows its mist.
    pub open: bool,
    /// Which layers' Objs go through, a bit a layer.
    pub passes: u32,
    /// Linear: the colour of its mist, which shows while it is closed or
    /// its far side is not made yet.
    pub mist: Vec3,
    /// How far from its opening a camera still sees its far side, in
    /// metres; `None` for the project's ([`PortalQuality::view_distance`]).
    /// Further off it shows its mist, and nothing of its far side is drawn.
    pub view_distance: Option<f32>,
}

impl Component for Portal {}

impl Default for Portal {
    fn default() -> Self {
        Self {
            size: Vec2::new(2.0, 3.0),
            to: None,
            open: true,
            passes: u32::MAX,
            mist: Vec3::new(0.35, 0.55, 1.0),
            view_distance: None,
        }
    }
}

impl Portal {
    /// An open Portal of `size` that leads to `to`.
    pub fn to(size: Vec2, to: Entity) -> Self {
        Self { size, to: Some(to), ..Self::default() }
    }

    /// How far from its opening a camera still sees its far side, in
    /// metres: its own view distance, or the project's.
    pub fn reach(&self, quality: &PortalQuality) -> f32 {
        self.view_distance.unwrap_or(quality.view_distance).max(0.0)
    }
}

/// How far past its view distance a Portal goes on showing its far side
/// once it has begun to, in metres: a camera that wanders back and forth
/// over the edge neither starts nor stops it.
pub fn reach_margin(reach: f32) -> f32 {
    (reach * 0.1).clamp(2.0, 16.0)
}

/// How much of a Portal's far side is there to show and to stand on, as the
/// engine finds each frame: without one, all of it while a camera is within
/// its view distance.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PortalFar {
    /// The ground round its exit is made, with what stands on it: always,
    /// where nothing streams.
    pub ready: bool,
    /// A camera is within its view distance: from when one comes within
    /// it until none is within it and [`reach_margin`] past.
    pub near: bool,
    /// How much of its far side shows through its mist, 0 to 1: it clears
    /// over a moment once the far side is ready, open and near, and
    /// thickens again as fast once it is not.
    pub shown: f32,
}

impl Component for PortalFar {}

/// Goes through Portals though it is neither a Character Controller nor a
/// Rigidbody, which go through without one: an Obj a script moves, as a
/// thrown thing with no body of its own.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PortalTraveller;

impl Component for PortalTraveller {}

/// On a camera's Obj: the Obj the camera films. While a Portal's opening
/// lies between the two, the camera's view is carried through it, so a
/// camera that follows something through a Portal goes on seeing it through
/// the opening until the camera crosses too. A camera under a Character
/// Controller or a Rigidbody films that without one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CameraFollows(pub Entity);

impl Component for CameraFollows {}

/// On a camera's Obj, set each frame by the engine: its view is carried
/// through a Portal, so it looks from where this takes its Obj, into the
/// dimension the Portal it looks out of is in.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewCarry(pub Carry, pub voxear_scene::DimensionId);

impl Component for ViewCarry {}

impl ViewCarry {
    /// Where a camera's Obj standing at `position`, turned `rotation`,
    /// looks from: carried, when it has one.
    pub fn of(world: &World, camera: Entity, position: Vec3, rotation: Quat) -> (Vec3, Quat) {
        match world.get::<ViewCarry>(camera) {
            Some(carry) => (carry.0.point(position), carry.0.turn(rotation)),
            None => (position, rotation),
        }
    }

    /// The dimension a camera sees: the one a Portal carries its view into,
    /// else its own.
    pub fn dimension(world: &World, camera: Entity) -> voxear_scene::DimensionId {
        match world.get::<ViewCarry>(camera) {
            Some(carry) => carry.1,
            None => voxear_scene::dimension_of(world, camera),
        }
    }
}

/// On an Obj that draws something, set each frame by the engine: it is
/// part-way through a Portal, and is drawn a second time where this takes
/// it, so what has passed the opening shows on the far side while the rest
/// still shows on the near one.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Straddles(pub Carry);

impl Component for Straddles {}

/// Where a Portal stands: its Obj's place and turn in the world, whatever
/// its scale.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PortalPose {
    pub position: Vec3,
    pub rotation: Quat,
}

impl PortalPose {
    pub fn of(transform: &Transform, world: Option<&WorldTransform>) -> Self {
        match world {
            Some(world) => {
                let (_, rotation, position) = world.0.to_scale_rotation_translation();
                Self { position, rotation }
            }
            None => Self { position: transform.position, rotation: transform.rotation },
        }
    }

    /// An Obj's pose, when it is there.
    pub fn of_obj(world: &World, entity: Entity) -> Option<Self> {
        let transform = world.get::<Transform>(entity)?;
        Some(Self::of(transform, world.get::<WorldTransform>(entity)))
    }

    /// A point of the world as the Portal has it: x across its opening, y
    /// up it, z out of its front.
    pub fn local(&self, point: Vec3) -> Vec3 {
        self.rotation.inverse() * (point - self.position)
    }

    /// The way its front faces.
    pub fn front(&self) -> Vec3 {
        self.rotation * Vec3::Z
    }

    /// Its opening's corners in the world: bottom left, bottom right, top
    /// right, top left, seen from its front.
    pub fn corners(&self, size: Vec2) -> [Vec3; 4] {
        let half = size.x * 0.5;
        [Vec3::new(half, 0.0, 0.0), Vec3::new(-half, 0.0, 0.0), Vec3::new(-half, size.y, 0.0), Vec3::new(half, size.y, 0.0)]
            .map(|corner| self.position + self.rotation * corner)
    }
}

/// What going through a Portal does to a place, a turn and a speed: turned
/// about the near Portal, then set down at the far one.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Carry {
    /// The near Portal's place.
    pub from: Vec3,
    /// The far Portal's.
    pub to: Vec3,
    /// The turn from the near side's directions to the far side's.
    pub rotation: Quat,
}

impl Carry {
    /// From the Portal standing at `from` to the one at `to`: in at the
    /// front of the first is out of the front of the second.
    pub fn between(from: &PortalPose, to: &PortalPose) -> Self {
        let half_turn = Quat::from_rotation_y(std::f32::consts::PI);
        let rotation = (to.rotation * half_turn * from.rotation.inverse()).normalize();
        Self { from: from.position, to: to.position, rotation }
    }

    /// A place on the near side, on the far side. Measured from the near
    /// Portal first, so a place close to it keeps its millimetres however
    /// far from the world's middle both Portals stand.
    pub fn point(&self, point: Vec3) -> Vec3 {
        self.to + self.rotation * (point - self.from)
    }

    /// A direction or a speed on the far side.
    pub fn direction(&self, direction: Vec3) -> Vec3 {
        self.rotation * direction
    }

    /// A turn on the far side.
    pub fn turn(&self, rotation: Quat) -> Quat {
        (self.rotation * rotation).normalize()
    }

    /// The way back.
    pub fn back(&self) -> Self {
        Self { from: self.to, to: self.from, rotation: self.rotation.inverse() }
    }

    /// The whole move as a matrix.
    pub fn matrix(&self) -> Mat4 {
        Mat4::from_translation(self.to) * Mat4::from_quat(self.rotation) * Mat4::from_translation(-self.from)
    }
}

/// Which face of a Portal something went in at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PortalSide {
    Front,
    Back,
}

/// Whether moving from `before` to `after` goes through the opening of the
/// Portal standing at `pose`, and in at which face. `margin` widens the
/// opening all round, for a body whose middle is not what is measured: feet
/// on its sill are level with its lower edge.
pub fn crossing(pose: &PortalPose, size: Vec2, before: Vec3, after: Vec3, margin: f32) -> Option<PortalSide> {
    let (a, b) = (pose.local(before), pose.local(after));
    let side = match (a.z > 0.0, b.z > 0.0) {
        (true, false) => PortalSide::Front,
        (false, true) if a.z < 0.0 => PortalSide::Back,
        _ => return None,
    };
    // Where the way between them meets the opening's plane.
    let along = a.z / (a.z - b.z);
    let at = a + (b - a) * along;
    let within = at.x.abs() <= size.x * 0.5 + margin && at.y >= -margin && at.y <= size.y + margin;
    within.then_some(side)
}

/// A Portal a view shows the far side of: where its opening is, what going
/// through it does, and its Obj and the one it leads to.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PortalSeen {
    pub entity: Entity,
    pub to: Entity,
    pub near: PortalPose,
    pub far: PortalPose,
    pub size: Vec2,
    pub carry: Carry,
    /// How much of the far side shows through its mist, and the mist's colour.
    pub shown: f32,
    pub mist: Vec3,
    /// How far the camera is from its opening.
    pub distance: f32,
    /// How much of the view its opening covers, 0 to 1.
    pub cover: f32,
    /// Its opening's rectangle in the view: its lowest and highest x and y,
    /// from -1 to 1.
    pub rect: [f32; 4],
    /// The dimension the Portal it leads to is in, which its far side shows.
    pub dimension: voxear_scene::DimensionId,
}

/// How far `point` is from the opening of the Portal at `pose`.
pub fn away(pose: &PortalPose, size: Vec2, point: Vec3) -> f32 {
    let local = pose.local(point);
    let nearest = Vec3::new(local.x.clamp(-size.x * 0.5, size.x * 0.5), local.y.clamp(0.0, size.y), 0.0);
    local.distance(nearest)
}

/// The rectangle of the view, from -1 to 1 each way, that the corners of an
/// opening cover through `view_proj`, cut where the opening passes behind
/// the camera; `None` when none of it is in view.
pub fn rect_in_view(corners: [Vec3; 4], view_proj: Mat4) -> Option<[f32; 4]> {
    // Just in front of the eye: what is nearer shows as wide as the view anyway.
    const NEAREST: f32 = 1e-4;
    let clip = corners.map(|corner| view_proj * corner.extend(1.0));
    let mut inside: Vec<Vec4> = Vec::with_capacity(8);
    for (index, &here) in clip.iter().enumerate() {
        let next = clip[(index + 1) % 4];
        if here.w >= NEAREST {
            inside.push(here);
        }
        if (here.w >= NEAREST) != (next.w >= NEAREST) {
            let along = (NEAREST - here.w) / (next.w - here.w);
            inside.push(here + (next - here) * along);
        }
    }
    if inside.is_empty() {
        return None;
    }
    let (mut low, mut high) = (Vec2::splat(f32::MAX), Vec2::splat(f32::MIN));
    for point in inside {
        let at = Vec2::new(point.x, point.y) / point.w;
        (low, high) = (low.min(at), high.max(at));
    }
    let (low, high) = (low.max(Vec2::splat(-1.0)), high.min(Vec2::splat(1.0)));
    (low.x < high.x && low.y < high.y).then_some([low.x, low.y, high.x, high.y])
}

/// The Portals in a view, as [`in_sight`] finds them.
#[derive(Clone, Debug, Default)]
pub struct PortalsInSight {
    /// Those whose far side the view draws, the nearest first.
    pub seen: Vec<PortalSeen>,
    /// Every opening the view draws: those in `seen` first, in their order,
    /// then those that show only their mist. Each with its pose, its size,
    /// its mist's colour and how much of the mist covers it.
    pub openings: Vec<(PortalPose, Vec2, Vec3, f32)>,
}

/// The Portals a camera at `eye` sees through `view_proj`: every one that
/// leads somewhere and has some of its opening in view. The far sides of
/// the nearest `most` are drawn, of those that are open, lead to a Portal
/// that is there, have their far side made and lie within their view
/// distance; the rest show their mist. `skip` leaves one out altogether:
/// the Portal a view through a Portal is itself looking out of.
///
/// The camera's `eye` and its `view_proj` are in the view's own space, whose
/// middle is `offset` from the world's.
pub fn in_sight(
    world: &World,
    (eye, offset): (Vec3, Vec3),
    view_proj: Mat4,
    most: usize,
    (skip, dimension): (Option<Entity>, voxear_scene::DimensionId),
) -> PortalsInSight {
    // Only the Portals of the dimension the view sees.
    let switched = Switched::in_dimension(world, dimension);
    let mut seen = Vec::new();
    let mut misted = Vec::new();
    let portals = world.query_ref::<(Entity, &Portal, &Transform, Option<&WorldTransform>)>().without::<Destroying>();
    for (entity, portal, transform, placed) in portals {
        // One that leads nowhere is only somewhere to come out, and shows nothing.
        if Some(entity) == skip || portal.to.is_none() || !switched.is_active(entity) {
            continue;
        }
        let near = PortalPose::of(transform, placed);
        let corners = near.corners(portal.size).map(|corner| corner - offset);
        let Some(rect) = rect_in_view(corners, view_proj) else { continue };
        let distance = away(&near, portal.size, eye + offset);
        match far_side(world, entity, portal, distance).filter(|(.., shown)| *shown > 0.0) {
            Some((to, far, shown)) => {
                let cover = (rect[2] - rect[0]) * (rect[3] - rect[1]) * 0.25;
                let carry = Carry::between(&near, &far);
                let (size, mist) = (portal.size, portal.mist);
                let dimension = voxear_scene::dimension_of(world, to);
                let seen_now =
                    PortalSeen { entity, to, near, far, size, carry, shown, mist, distance, cover, rect, dimension };
                seen.push(seen_now);
            }
            None => misted.push((near, portal.size, portal.mist, 1.0)),
        }
    }
    seen.sort_by(|a, b| a.distance.total_cmp(&b.distance));
    // Those past the most a view draws show their mist.
    let most = most.min(MAX_PORTAL_QUADS);
    let past = seen.split_off(most.min(seen.len()));
    let mut openings: Vec<_> = seen.iter().map(|seen| (seen.near, seen.size, seen.mist, 1.0 - seen.shown)).collect();
    openings.extend(past.iter().map(|seen| (seen.near, seen.size, seen.mist, 1.0)));
    openings.extend(misted);
    openings.truncate(MAX_PORTAL_QUADS);
    PortalsInSight { seen, openings }
}

/// The Portal `portal` leads to, where it stands, and how much of it shows
/// from `distance` away: `None` unless it leads to a Portal that is there.
/// What shows is what the engine has cleared of its mist (its
/// [`PortalFar`]), which it thickens again once no camera is within the
/// view distance and its margin; and from this view, as from a second
/// camera further off, nothing once it is a margin further again, thinning
/// over that. Without a `PortalFar`, as where no engine runs, all of it to
/// the view distance and its margin.
fn far_side(world: &World, entity: Entity, portal: &Portal, distance: f32) -> Option<(Entity, PortalPose, f32)> {
    let to = portal.to.filter(|to| *to != entity && world.has::<Portal>(*to) && !world.has::<Destroying>(*to))?;
    let far = PortalPose::of_obj(world, to)?;
    let quality = world.resource::<PortalQuality>().copied().unwrap_or_default();
    let reach = portal.reach(&quality);
    let margin = reach_margin(reach);
    let cleared = world.get::<PortalFar>(entity).map_or(1.0, |far| far.shown);
    let within = ((reach + margin * 2.0 - distance) / margin).clamp(0.0, 1.0);
    let shown = if portal.open { cleared.clamp(0.0, 1.0) * within } else { 0.0 };
    Some((to, far, shown))
}

/// The view through a Portal, for the camera looking at it: a camera of its
/// own on the far side, set where going through takes the first, in a space
/// of its own whose middle is a whole number of metres from the world's, by
/// the far Portal, so its numbers stay small however far off that is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Through {
    /// The camera's place and turn, in the view's own space.
    pub position: Vec3,
    pub rotation: Quat,
    /// Where the view's own space has its middle, in the world's: whole
    /// metres.
    pub offset: Vec3,
    /// What it draws lies within these, inward, in its own space: the sides
    /// of its opening's rectangle in the view, and the far Portal's plane.
    pub planes: [[f32; 4]; 6],
    /// The far Portal's plane, in its own space: nothing nearer the camera
    /// than it is drawn.
    pub cut: [f32; 4],
    /// The Portal it looks out of, which it does not draw as one.
    pub out_of: Entity,
    /// Where that Portal stands, in the view's own space: all the view
    /// sees, it sees through there.
    pub window: Vec3,
    /// The dimension it sees: the far Portal's.
    pub dimension: voxear_scene::DimensionId,
}

impl Through {
    /// The view through `seen` for a camera `from_near` from the near
    /// Portal's place, turned `rotation` in the world, whose projection
    /// without jitter is `projection`, drawing no further than `far` metres
    /// ahead where that is given.
    pub fn new(seen: &PortalSeen, from_near: Vec3, rotation: Quat, projection: Mat4, far: Option<f32>) -> Self {
        let offset = seen.far.position.round();
        // From the near Portal, then from the far one's own middle: small numbers all the way.
        let position = (seen.far.position - offset) + seen.carry.direction(from_near);
        let rotation = seen.carry.turn(rotation);
        // The far Portal's plane, facing away from the camera.
        let front = seen.far.front();
        let on_plane = seen.far.position - offset;
        let facing = if front.dot(position - on_plane) > 0.0 { -front } else { front };
        let cut = [facing.x, facing.y, facing.z, -facing.dot(on_plane)];
        let view = Mat4::from_rotation_translation(rotation, position).inverse();
        let unproject = (projection * view).inverse();
        let [x0, y0, x1, y1] = seen.rect;
        // A ray of the view through a point of it, in the view's own space.
        let ray = |x: f32, y: f32| {
            let far = unproject * Vec4::new(x, y, 0.5, 1.0);
            (far.truncate() / far.w - position).normalize_or_zero()
        };
        let middle = ray((x0 + x1) * 0.5, (y0 + y1) * 0.5);
        // The plane through the camera and two of the rectangle's corners, facing its middle.
        let side = |a: Vec3, b: Vec3| {
            let normal = a.cross(b).normalize_or_zero();
            let normal = if normal.dot(middle) < 0.0 { -normal } else { normal };
            [normal.x, normal.y, normal.z, -normal.dot(position)]
        };
        let corners = [ray(x0, y0), ray(x1, y0), ray(x1, y1), ray(x0, y1)];
        let planes = [
            side(corners[3], corners[0]),
            side(corners[1], corners[2]),
            side(corners[0], corners[1]),
            side(corners[2], corners[3]),
            cut,
            // As far as it is drawn, where it is not drawn as far as the world.
            match far {
                Some(far) => {
                    let ahead = rotation * Vec3::Z;
                    [-ahead.x, -ahead.y, -ahead.z, ahead.dot(position) + far]
                }
                None => [0.0, 0.0, 0.0, 1.0],
            },
        ];
        Self { position, rotation, offset, planes, cut, out_of: seen.to, window: on_plane, dimension: seen.dimension }
    }
}

/// A Portal's opening as `shaders/portal.slang` draws it (`PortalQuad` in
/// `shaders/common.slang`): its corners as the view sees them, and its mist.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuPortalQuad {
    pub corners: [[f32; 4]; 4],
    /// The mist's colour, and how much of it covers the far side.
    pub mist: [f32; 4],
}

/// The most openings a view draws, with those it only shows the mist of.
pub const MAX_PORTAL_QUADS: usize = 16;

impl GpuPortalQuad {
    /// The opening at `pose` as a view sees it through `view_proj`, whose
    /// space has its middle `offset` from the world's, its mist of `colour`
    /// covering `mist` of it.
    pub fn new(pose: &PortalPose, size: Vec2, (offset, view_proj): (Vec3, Mat4), colour: Vec3, mist: f32) -> Self {
        let corners = pose.corners(size).map(|corner| (view_proj * (corner - offset).extend(1.0)).to_array());
        Self { corners, mist: [colour.x, colour.y, colour.z, mist] }
    }
}

/// How much a game spends on Portals, as a world's resource: without one,
/// the default.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PortalQuality {
    /// How many far sides a view draws at most, the nearest first. The rest
    /// show their mist.
    pub views: u32,
    /// How many Portals deep far sides are drawn: 1 shows a Portal seen
    /// through a Portal as its mist, 2 shows its far side too.
    pub depth: u32,
    /// The sun casts shadows on a far side while the camera is within this
    /// many metres of the opening; 0 for never.
    pub shadow_distance: f32,
    /// How far round a Portal's far side a world without end makes its own
    /// blocks, in metres: never further than round the camera. Its far
    /// view reaches from there as far as the world's own does.
    pub far_side_distance: f32,
    /// Whether the water of a far side's worlds of blocks is drawn. It
    /// costs about as much again as the water of the camera's own view
    /// while any shows through an opening.
    pub water: bool,
    /// How far ahead a far side is drawn, in metres; 0 for as far as the
    /// world is. Past it shows the sky.
    pub draw_distance: f32,
    /// How far from its opening a camera sees a Portal's far side, in
    /// metres, for each Portal without a view distance of its own.
    pub view_distance: f32,
}

impl Default for PortalQuality {
    fn default() -> Self {
        Self {
            views: 4,
            depth: 1,
            shadow_distance: 30.0,
            far_side_distance: 96.0,
            water: true,
            draw_distance: 0.0,
            view_distance: 96.0,
        }
    }
}

/// The deepest far sides are ever drawn, whatever a game asks for.
pub const MAX_PORTAL_DEPTH: u32 = 2;

#[cfg(test)]
mod tests {
    use voxear_core::math::camera::lh::proj::vulkan;

    use super::*;

    fn close(a: Vec3, b: Vec3) -> bool {
        a.distance(b) < 1e-3
    }

    #[test]
    fn going_in_at_the_front_of_one_comes_out_of_the_front_of_the_other() {
        // One facing +z at the middle, the other 100 m east and facing east.
        let near = PortalPose { position: Vec3::ZERO, rotation: Quat::IDENTITY };
        let far = PortalPose { position: Vec3::new(100.0, 5.0, 0.0), rotation: Quat::from_rotation_y(std::f32::consts::FRAC_PI_2) };
        let carry = Carry::between(&near, &far);
        // A step in front of the near one, a little to its left and up, walking in: on the far side a step behind
        // it, mirrored across it, walking out of its front.
        let (place, speed) = (Vec3::new(0.5, 1.0, 1.0), Vec3::new(0.0, 0.0, -2.0));
        let landed = carry.point(place);
        assert!(close(far.local(landed), Vec3::new(-0.5, 1.0, -1.0)), "{:?}", far.local(landed));
        assert!(close(carry.direction(speed), far.front() * 2.0), "{:?}", carry.direction(speed));
        // Up stays up, and the way back undoes it.
        assert!(close(carry.direction(Vec3::Y), Vec3::Y));
        assert!(close(carry.back().point(landed), place));
        assert!(close(carry.matrix().transform_point3(place), landed));
        // Facing the near one, it faces out of the far one.
        let facing = carry.turn(Quat::from_rotation_y(std::f32::consts::PI));
        assert!(close(facing * Vec3::Z, far.front()));
    }

    #[test]
    fn a_place_by_a_portal_keeps_its_millimetres_far_from_the_worlds_middle() {
        let near = PortalPose { position: Vec3::new(1500.25, 64.0, -900.5), rotation: Quat::from_rotation_y(0.7) };
        let far = PortalPose { position: Vec3::new(48_000.0, 80.0, 31_000.0), rotation: Quat::from_rotation_y(-2.1) };
        let carry = Carry::between(&near, &far);
        // Two places a millimetre apart land a millimetre apart, not on the same 4 mm step.
        let step = near.rotation * Vec3::new(0.001, 0.0, 0.0);
        let a = carry.point(near.position + near.rotation * Vec3::new(0.3, 1.2, 0.8)) - far.position;
        let b = carry.point(near.position + near.rotation * Vec3::new(0.3, 1.2, 0.8) + step) - far.position;
        assert!(((a - b).length() - 0.001).abs() < 0.0045, "{}", (a - b).length());
        assert!((far.rotation.inverse() * a - Vec3::new(-0.3, 1.2, -0.8)).length() < 0.01);
    }

    #[test]
    fn only_a_way_through_the_opening_crosses_it() {
        let pose = PortalPose { position: Vec3::new(10.0, 2.0, 0.0), rotation: Quat::IDENTITY };
        let size = Vec2::new(2.0, 3.0);
        let through = |before: [f32; 3], after: [f32; 3]| crossing(&pose, size, before.into(), after.into(), 0.0);
        assert_eq!(through([10.0, 3.0, 0.5], [10.0, 3.0, -0.5]), Some(PortalSide::Front));
        assert_eq!(through([10.0, 3.0, -0.5], [10.0, 3.0, 0.5]), Some(PortalSide::Back));
        // Past its side, over its top, and under its sill.
        assert_eq!(through([11.2, 3.0, 0.5], [11.2, 3.0, -0.5]), None);
        assert_eq!(through([10.0, 5.2, 0.5], [10.0, 5.2, -0.5]), None);
        assert_eq!(through([10.0, 1.9, 0.5], [10.0, 1.9, -0.5]), None);
        // Feet on the sill go through with a margin.
        assert_eq!(crossing(&pose, size, Vec3::new(10.0, 1.95, 0.5), Vec3::new(10.0, 1.95, -0.5), 0.1), Some(PortalSide::Front));
        // Staying on one side, and going through aslant: it is where the way meets the plane that counts.
        assert_eq!(through([10.0, 3.0, 0.5], [10.0, 3.0, 0.2]), None);
        assert_eq!(through([8.0, 3.0, 1.0], [11.0, 3.0, -0.5]), Some(PortalSide::Front));
        assert_eq!(through([4.0, 3.0, 1.0], [8.5, 3.0, -0.5]), None);
        // Starting on the plane itself is not a crossing: it was counted as it arrived.
        assert_eq!(through([10.0, 3.0, 0.0], [10.0, 3.0, 0.5]), None);
    }

    #[test]
    fn the_view_through_a_portal_looks_out_of_the_far_one_at_what_is_in_front_of_it() {
        let near = PortalPose { position: Vec3::new(0.0, 0.0, 10.0), rotation: Quat::from_rotation_y(std::f32::consts::PI) };
        let far = PortalPose { position: Vec3::new(30_000.4, 12.0, -7_000.6), rotation: Quat::from_rotation_y(0.5) };
        let size = Vec2::new(2.0, 3.0);
        let carry = Carry::between(&near, &far);
        // A camera 4 m in front of the near one, at eye height, looking at it along +z.
        let eye = Vec3::new(0.0, 1.6, 6.0);
        let turned = Quat::IDENTITY;
        let projection = vulkan::perspective_infinite_reverse(1.0, 16.0 / 9.0, 0.1);
        let view = Mat4::from_rotation_translation(turned, eye).inverse();
        let rect = rect_in_view(near.corners(size), projection * view).expect("in view");
        assert!(rect[0] < 0.0 && rect[2] > 0.0 && rect[0] > -0.5 && rect[2] < 0.5, "{rect:?}");
        // Looking the other way, none of it is in view.
        let away = Mat4::from_rotation_translation(Quat::from_rotation_y(std::f32::consts::PI), eye).inverse();
        assert_eq!(rect_in_view(near.corners(size), projection * away), None);
        let mut world = World::new();
        let (entity, to) = (world.spawn(()), world.spawn(()));
        let seen = PortalSeen {
            entity,
            to,
            near,
            far,
            size,
            carry,
            shown: 1.0,
            mist: Vec3::ONE,
            distance: 4.0,
            cover: 0.1,
            rect,
            dimension: voxear_scene::DimensionId::LEVEL,
        };
        let through = Through::new(&seen, eye - near.position, turned, projection, None);
        assert_eq!(through.offset, Vec3::new(30_000.0, 12.0, -7_001.0));
        // The camera stands 4 m behind the far one, in a space whose numbers are small.
        let behind = far.position - through.offset - far.front() * 4.0 + Vec3::Y * 1.6;
        assert!(close(through.position, behind), "{:?} {behind:?}", through.position);
        let inside = |point: Vec3| through.planes.iter().all(|p| Vec3::new(p[0], p[1], p[2]).dot(point) + p[3] >= 0.0);
        let own = |local: Vec3| far.position - through.offset + far.rotation * local;
        // What stands in front of the far one, seen through its opening, is drawn; what stands between the camera
        // and it, behind its plane, is not; nor what is out past the opening's sides.
        assert!(inside(own(Vec3::new(0.0, 1.5, 3.0))));
        assert!(!inside(own(Vec3::new(0.0, 1.5, -2.0))));
        assert!(!inside(own(Vec3::new(9.0, 1.5, 3.0))));
        assert!(inside(own(Vec3::new(0.0, 1.5, 0.01))) && !inside(own(Vec3::new(0.0, 1.5, -0.01))));
    }

    #[test]
    fn an_opening_partly_behind_the_camera_still_covers_what_shows_of_it() {
        let pose = PortalPose { position: Vec3::ZERO, rotation: Quat::IDENTITY };
        let projection = vulkan::perspective_infinite_reverse(1.0, 1.0, 0.1);
        // Standing in the opening looking along it: one side of it runs off behind the camera.
        let eye = Vec3::new(0.0, 1.5, 0.05);
        let view = Mat4::from_rotation_translation(Quat::from_rotation_y(std::f32::consts::FRAC_PI_2), eye).inverse();
        let rect = rect_in_view(pose.corners(Vec2::new(2.0, 3.0)), projection * view).expect("in view");
        assert!(rect[3] - rect[1] > 1.9, "as tall as the view: {rect:?}");
    }
}
