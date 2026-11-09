//! The Scene view's tools, as Unity's: click an Obj to select it, and drag
//! the handles around the selected one to move, rotate or scale it. Handles
//! are drawn as Voxear UI shapes over the 3D view, projected through the
//! same camera the renderer uses, and stay the same size on screen.
//!
//! Move has an arrow per axis, a square between each two axes that moves
//! along both, and a dot in the middle that moves in the view's plane.
//! Rotate has a ring per axis, an outer ring that turns around the
//! direction the view looks, and turns freely, as a trackball, when dragged
//! inside the rings. Scale has an axis per side, a square between each two
//! that scales both together, and a middle that scales evenly. The squares
//! sit on the side of each axis that faces the camera, and hide when seen
//! edge on. Axes are the world's (Global) or the Obj's own (Local); handles
//! sit at the Obj's pivot or the middle of its bounds. Holding Ctrl while
//! dragging snaps: to the snap distance, to 15 degrees, or to tenths of the
//! scale.

use voxear_core::math::{Mat4, Quat, Vec2, Vec3, Vec4, camera::lh::proj::vulkan};
use voxear_ecs::{Entity, World};
use voxear_engine::audio::AudioSource;
use voxear_render::{Camera, Foliage, Light, MeshHandle, MeshRenderer, ModelRenderer, Renderer};
use voxear_scene::{Destroying, EditorOnly, Parent, Switch, Switched, Transform, WorldTransform};
use voxear_ui::{Color, Shape};

/// How long the handles are on screen, in logical pixels.
const HANDLE_LENGTH: f32 = 84.0;
/// How near the pointer must be to a handle to take it.
const REACH: f32 = 8.0;
/// How near to a light or camera a click selects it, as they have no mesh.
const ICON_REACH: f32 = 12.0;
const RING_SEGMENTS: usize = 48;
/// How long the move arrowheads and how big the scale blocks are, on screen.
const ARROW_HEAD: f32 = 12.0;
const BLOCK: f32 = 9.0;
/// Where the two-axis squares start and end along each of their axes, as a
/// share of the handles' length.
const PLANE_NEAR: f32 = 0.2;
const PLANE_FAR: f32 = 0.45;
/// A square seen more edge on than this, as the cosine of the angle between
/// its axis and the view, hides: it could not be told apart or held.
const PLANE_EDGE: f32 = 0.2;
/// How far out rotate's view ring is, past the axis rings, on screen.
const VIEW_RING: f32 = HANDLE_LENGTH * 1.08;
/// How far the axis rings are, as a share of the handles' length.
const AXIS_RING: f32 = 0.9;
/// How far turning freely turns for each logical pixel dragged, in radians.
const FREE_TURN: f32 = 0.01;
/// How far away the ground can be met to place something on it; further,
/// the pointer is too near the horizon for it to be where it was meant.
const GROUND_REACH: f32 = 200.0;

/// How far along a ray from the Scene view's camera a terrain is looked
/// for, in metres.
const TERRAIN_REACH: f32 = 20_000.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tool {
    #[default]
    Select,
    Move,
    Rotate,
    Scale,
}

impl Tool {
    pub fn from_index(index: usize) -> Self {
        match index {
            1 => Tool::Move,
            2 => Tool::Rotate,
            3 => Tool::Scale,
            _ => Tool::Select,
        }
    }
}

/// A part of the handles the pointer can take.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Handle {
    Axis(usize),
    /// The square across this axis, for moving or scaling along the other two.
    Plane(usize),
    Middle,
    /// Rotate's outer ring, which turns around the direction the view looks.
    View,
    /// Inside rotate's rings, which turns freely, as a trackball.
    Free,
}

/// The Scene camera as the renderer sees it, over the view's rectangle in
/// logical pixels: rays through points, and points onto the screen.
#[derive(Clone, Copy, Debug)]
pub struct ViewCamera {
    view_projection: Mat4,
    inverse: Mat4,
    pub position: Vec3,
    pub forward: Vec3,
    /// Which way the view's right and up are, in the world.
    pub right: Vec3,
    pub up: Vec3,
    /// Left, top, width and height.
    pub rect: [f32; 4],
    /// World units per logical pixel one unit in front of the camera.
    spread: f32,
}

impl ViewCamera {
    pub fn new(world: &World, camera: Entity, rect: [f32; 4]) -> Option<Self> {
        let lens = world.get::<Camera>(camera)?;
        let transform = world.get::<Transform>(camera)?;
        let fov = lens.fov_y_degrees.to_radians();
        let aspect = rect[2] / rect[3].max(1.0);
        let view = Mat4::from_rotation_translation(transform.rotation, transform.position).inverse();
        let view_projection = vulkan::perspective_infinite_reverse(fov, aspect, lens.near) * view;
        Some(Self {
            view_projection,
            inverse: view_projection.inverse(),
            position: transform.position,
            forward: transform.rotation * Vec3::Z,
            right: transform.rotation * Vec3::X,
            up: transform.rotation * Vec3::Y,
            rect,
            spread: 2.0 * (fov * 0.5).tan() / rect[3].max(1.0),
        })
    }

    /// Where a point in the world shows on screen, or `None` behind the camera.
    pub fn project(&self, point: Vec3) -> Option<[f32; 2]> {
        let clip = self.view_projection * point.extend(1.0);
        if clip.w <= 1e-4 {
            return None;
        }
        let (x, y) = (clip.x / clip.w, clip.y / clip.w);
        Some([self.rect[0] + (x + 1.0) * 0.5 * self.rect[2], self.rect[1] + (y + 1.0) * 0.5 * self.rect[3]])
    }

    /// The ray from the camera through a point on screen.
    pub fn ray(&self, point: [f32; 2]) -> (Vec3, Vec3) {
        let x = (point[0] - self.rect[0]) / self.rect[2] * 2.0 - 1.0;
        let y = (point[1] - self.rect[1]) / self.rect[3] * 2.0 - 1.0;
        // Reverse depth: 1 is the near plane, and the far one is at infinity.
        let unproject = |depth: f32| {
            let p = self.inverse * Vec4::new(x, y, depth, 1.0);
            p.truncate() / p.w
        };
        let (near, further) = (unproject(1.0), unproject(0.5));
        (near, (further - near).normalize_or(self.forward))
    }

    /// World units a number of logical pixels spans at a point.
    pub fn world_size(&self, point: Vec3, pixels: f32) -> f32 {
        (point - self.position).dot(self.forward).max(0.01) * self.spread * pixels
    }
}

/// Which markers show, and can be clicked: lights', cameras', both or neither.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Markers {
    pub lights: bool,
    pub cameras: bool,
    /// Audio Sources, and while the game plays, which of them are playing.
    pub sounds: bool,
    /// The selected Obj's colliders.
    pub colliders: bool,
}

impl Markers {
    pub const NONE: Self = Self { lights: false, cameras: false, sounds: false, colliders: false };

    /// Whether an Obj with a light, a camera or an Audio Source has a
    /// marker that shows.
    pub fn show(self, light: bool, camera: bool, sound: bool) -> bool {
        (light && self.lights) || (camera && self.cameras) || (sound && self.sounds)
    }
}

/// The Obj under a point in the Scene view: the nearest mesh or UI Panel
/// the ray meets, or a light, camera or Audio Source whose marker shows
/// close by.
pub fn pick(
    world: &World,
    renderer: &Renderer,
    camera: &ViewCamera,
    point: [f32; 2],
    markers: Markers,
) -> Option<Entity> {
    let (origin, direction) = camera.ray(point);
    let mut best: Option<(f32, Entity)> = None;
    let mut consider = |distance: f32, entity: Entity| {
        if best.is_none_or(|(nearest, _)| distance < nearest) {
            best = Some((distance, entity));
        }
    };
    // A plant of a Foliage picks the Foliage's Obj, as grass picks a terrain.
    for (distance, _, entity) in mesh_hits(world, renderer, (origin, direction), true) {
        consider(distance, entity);
    }
    // A UI Panel by its rectangle, from the side it shows on.
    for (entity, rect, _) in voxear_render::ui_panel_rects(world, camera.position) {
        if let Some((along, _)) = rect.hit(origin, direction).filter(|_| !world.has::<EditorOnly>(entity)) {
            consider(along, entity);
        }
    }
    let marked = world
        .query_ref::<(Entity, &Transform, Option<&WorldTransform>, Option<&Light>, Option<&Camera>, Option<&AudioSource>)>()
        .without::<EditorOnly>()
        .without::<Destroying>();
    // An Obj that is off shows no icon to click, as in Unity.
    let switched = Switched::of(world);
    for (entity, transform, placed, light, lens, sound) in marked {
        if !markers.show(light.is_some(), lens.is_some(), sound.is_some()) || !switched.is_active(entity) {
            continue;
        }
        let at = placed.map_or(transform.position, |w| w.0.w_axis.truncate());
        if let Some(screen) = camera.project(at)
            && (screen[0] - point[0]).hypot(screen[1] - point[1]) <= ICON_REACH
        {
            consider(at.distance(origin), entity);
        }
    }
    best.map(|(_, entity)| entity)
}

/// Where the ray through a point in the Scene view first meets a mesh, or
/// else the ground, the plane through the origin facing up, if
/// that is in front and not too near the horizon to place things on.
pub fn surface_at(world: &World, renderer: &Renderer, camera: &ViewCamera, point: [f32; 2]) -> Option<Vec3> {
    surface_past(world, renderer, camera, point, &[])
}

/// [`surface_at`], passing through the Objs in `skip`, as what is being
/// placed under the pointer must not land on itself.
pub fn surface_past(
    world: &World,
    renderer: &Renderer,
    camera: &ViewCamera,
    point: [f32; 2],
    skip: &[Entity],
) -> Option<Vec3> {
    let ray = camera.ray(point);
    // Not on top of grass and plants, but on the ground among them.
    let hits = mesh_hits(world, renderer, ray, false).into_iter().filter(|(_, _, obj)| !skip.contains(obj));
    let nearest = hits.min_by(|a, b| a.0.total_cmp(&b.0));
    if let Some((_, hit, _)) = nearest {
        return Some(hit);
    }
    let ground = on_plane(ray, Vec3::ZERO, Vec3::Y)?;
    let along = (ground - ray.0).dot(ray.1);
    (along > 0.0 && along < GROUND_REACH).then_some(ground)
}

/// The ground straight below `(x, z)`, from `top` metres up down, on any
/// mesh or terrain but plants and the Objs in `skip`: where it is, and the
/// way it faces there, from the ground half a metre round.
pub(crate) fn ground_below(
    world: &World,
    renderer: &Renderer,
    (x, z, top): (f32, f32, f32),
    skip: &[Entity],
) -> Option<(Vec3, Vec3)> {
    let down = |x: f32, z: f32| {
        let ray = (Vec3::new(x, top, z), Vec3::NEG_Y);
        let hits = mesh_hits(world, renderer, ray, false).into_iter().filter(|(_, _, obj)| !skip.contains(obj));
        hits.min_by(|a, b| a.0.total_cmp(&b.0)).map(|(_, hit, _)| hit)
    };
    let at = down(x, z)?;
    const ROUND: f32 = 0.5;
    let [east, west, north, south] = [(ROUND, 0.0), (-ROUND, 0.0), (0.0, ROUND), (0.0, -ROUND)]
        .map(|(dx, dz)| down(x + dx, z + dz).filter(|hit| (hit.y - at.y).abs() < 4.0).unwrap_or(at));
    let normal = (north - south).cross(east - west).normalize_or(Vec3::Y);
    Some((at, if normal.y < 0.0 { -normal } else { normal }))
}

/// Each mesh a ray meets: how far along the ray, where, and its Obj. A ray
/// that enters a mesh's box is tried against its triangles, as a box alone
/// let a building's parts, which the camera stands inside, take every click.
/// With `plants`, each plant of a Foliage too, by its own box, for its Obj.
fn mesh_hits(
    world: &World,
    renderer: &Renderer,
    (origin, direction): (Vec3, Vec3),
    plants: bool,
) -> Vec<(f32, Vec3, Entity)> {
    let meshes = world
        .query_ref::<(Entity, &MeshRenderer, &Transform, Option<&WorldTransform>)>()
        .without::<EditorOnly>()
        .without::<Destroying>();
    // What is not drawn is not hit.
    let switched = Switched::of(world);
    let drawn = |entity: Entity, switch: Switch| switched.is_on(world, entity, switch);
    let mut hits = Vec::new();
    let mut try_box = |entity: Entity, matrix: Mat4, (min, max): (Vec3, Vec3), mesh: Option<MeshHandle>| {
        let inverse = matrix.inverse();
        let (local_origin, local_direction) = (inverse.transform_point3(origin), inverse.transform_vector3(direction));
        let Some(t) = enter_box(local_origin, local_direction, min, max) else { return };
        let t = match mesh.and_then(|mesh| renderer.mesh_triangles(mesh)) {
            Some(triangles) => match triangles.first_hit(local_origin, local_direction) {
                Some(t) => t,
                None => return,
            },
            None => t,
        };
        let hit = matrix.transform_point3(local_origin + local_direction * t);
        hits.push((hit.distance(origin), hit, entity));
    };
    for (entity, renderer_of, transform, placed) in meshes.into_iter().filter(|(e, ..)| drawn(*e, Switch::Renderer)) {
        let Some(bounds) = renderer.mesh_bounds(renderer_of.mesh) else { continue };
        try_box(entity, placed.map_or_else(|| transform.matrix(), |w| w.0), bounds, Some(renderer_of.mesh));
    }
    // A model drawn whole is hit by each of its parts' boxes, not the box round them all, which would take in
    // everything inside a building.
    let models = world
        .query_ref::<(Entity, &ModelRenderer, &Transform, Option<&WorldTransform>)>()
        .without::<EditorOnly>()
        .without::<Destroying>();
    for (entity, model, transform, placed) in models.into_iter().filter(|(e, ..)| drawn(*e, Switch::Model)) {
        let obj = placed.map_or_else(|| transform.matrix(), |w| w.0);
        for part in model.parts.iter() {
            let Some(bounds) = renderer.mesh_bounds(part.mesh) else { continue };
            try_box(entity, obj * part.transform, bounds, Some(part.mesh));
        }
    }
    // A terrain by its ground's own triangles, which are what is drawn near the camera.
    let mut grounds = Vec::new();
    for (entity, terrain, corner) in voxear_terrain::terrains(world) {
        if world.has::<EditorOnly>(entity) {
            continue;
        }
        let local = origin - corner;
        if let Some(hit) = voxear_terrain::raycast_terrain(terrain, local, direction, TERRAIN_REACH) {
            grounds.push((hit.distance, hit.point + corner, entity));
        }
    }
    let foliage = world
        .query_ref::<(Entity, &Foliage, &voxear_scene::Plants, &Transform, Option<&WorldTransform>)>()
        .without::<EditorOnly>()
        .without::<Destroying>();
    let foliage = foliage.into_iter().filter(|(e, ..)| plants && drawn(*e, Switch::Foliage));
    for (entity, foliage, grown, transform, placed) in foliage {
        let Some(bounds) = renderer.mesh_bounds(foliage.mesh) else { continue };
        let obj = placed.map_or_else(|| transform.matrix(), |w| w.0);
        for (_, plant) in grown.standing() {
            try_box(entity, plant.matrix(obj), bounds, None);
        }
    }
    hits.extend(grounds);
    hits
}

/// How far along a ray it enters a box, by the slab method; `None` if it
/// misses or the box is behind.
fn enter_box(origin: Vec3, direction: Vec3, min: Vec3, max: Vec3) -> Option<f32> {
    let (mut near, mut far) = (f32::MIN, f32::MAX);
    for axis in 0..3 {
        let (o, d) = (origin[axis], direction[axis]);
        if d.abs() < 1e-8 {
            if o < min[axis] || o > max[axis] {
                return None;
            }
            continue;
        }
        let (a, b) = ((min[axis] - o) / d, (max[axis] - o) / d);
        near = near.max(a.min(b));
        far = far.min(a.max(b));
    }
    (near <= far && far >= 0.0).then_some(near.max(0.0))
}

/// Where on a line through `point` along `axis` comes nearest a ray, as a
/// distance along the axis; `None` when they run side by side.
pub(crate) fn along_axis((origin, direction): (Vec3, Vec3), point: Vec3, axis: Vec3) -> Option<f32> {
    let between = origin - point;
    let b = direction.dot(axis);
    let denominator = 1.0 - b * b;
    if denominator.abs() < 1e-5 {
        return None;
    }
    Some((between.dot(axis) - b * between.dot(direction)) / denominator)
}

/// Where a ray crosses a plane, or `None` when it runs along it.
pub(crate) fn on_plane((origin, direction): (Vec3, Vec3), point: Vec3, normal: Vec3) -> Option<Vec3> {
    let facing = direction.dot(normal);
    if facing.abs() < 1e-5 {
        return None;
    }
    Some(origin + direction * ((point - origin).dot(normal) / facing))
}

/// A drag under way: the handle, and everything as it was when it began.
#[derive(Clone, Copy, Debug)]
struct Drag {
    entity: Entity,
    handle: Handle,
    start: Transform,
    /// The handles' middle and axes then, in the world.
    middle: Vec3,
    axes: [Vec3; 3],
    /// Where the pointer first took hold: along the axis, on the plane, or on screen.
    along: f32,
    grip: Vec3,
    pointer: [f32; 2],
    length: f32,
    /// The Obj's world rotation then, and its parent's world transform.
    world_rotation: Quat,
    parent: Mat4,
    /// The view's right, up and forward then, for turning with the view.
    view: [Vec3; 3],
    /// For a scale square: the way out from the middle through it.
    outward: Vec3,
}

impl Drag {
    /// The Obj's rotation, as its parent holds it, once turned by `turn` in
    /// the world from where it was when the drag began.
    fn turned(&self, turn: Quat) -> Quat {
        let parent_rotation = self.parent.to_scale_rotation_translation().1;
        (parent_rotation.inverse() * (turn * self.world_rotation)).normalize()
    }
}

/// The tool in use, its settings, and any drag under way.
#[derive(Default)]
pub struct SceneTools {
    pub tool: Tool,
    /// The Obj's own axes rather than the world's.
    pub local: bool,
    /// Handles at the middle of the Obj's bounds rather than its pivot.
    pub centred: bool,
    /// What a move snaps to while Ctrl is held, in world units.
    pub snap: f32,
    drag: Option<Drag>,
    hovered: Option<Handle>,
}

impl SceneTools {
    pub fn new() -> Self {
        Self { snap: 0.25, ..Self::default() }
    }

    pub fn is_dragging(&self) -> bool {
        self.drag.is_some()
    }

    /// The handles' middle and axes for `entity`, in the world, and the
    /// world length they span on screen.
    fn frame(
        &self,
        world: &World,
        renderer: &Renderer,
        camera: &ViewCamera,
        entity: Entity,
    ) -> Option<(Vec3, [Vec3; 3], f32)> {
        let transform = world.get::<Transform>(entity)?;
        let matrix = world.get::<WorldTransform>(entity).map_or_else(|| transform.matrix(), |w| w.0);
        let (_, rotation, position) = matrix.to_scale_rotation_translation();
        let middle = match (self.centred, world.get::<MeshRenderer>(entity).and_then(|m| renderer.mesh_bounds(m.mesh)))
        {
            (true, Some((min, max))) => matrix.transform_point3((min + max) * 0.5),
            _ => position,
        };
        // Scale always works along the Obj's own axes.
        let own = self.local || self.tool == Tool::Scale;
        let axes = if own {
            [rotation * Vec3::X, rotation * Vec3::Y, rotation * Vec3::Z]
        } else {
            [Vec3::X, Vec3::Y, Vec3::Z]
        };
        Some((middle, axes, camera.world_size(middle, HANDLE_LENGTH)))
    }

    /// The handle under a point, if the tool has handles and one is there.
    pub fn handle_at(
        &self,
        world: &World,
        renderer: &Renderer,
        camera: &ViewCamera,
        entity: Entity,
        point: [f32; 2],
    ) -> Option<Handle> {
        if self.tool == Tool::Select {
            return None;
        }
        self.handle_in(camera, self.frame(world, renderer, camera, entity)?, point)
    }

    /// The handle under a point, for handles whose middle, axes and length
    /// are as given.
    fn handle_in(
        &self,
        camera: &ViewCamera,
        (middle, axes, length): (Vec3, [Vec3; 3], f32),
        point: [f32; 2],
    ) -> Option<Handle> {
        let centre = camera.project(middle)?;
        if self.tool != Tool::Rotate && distance(point, centre) <= REACH + 3.0 {
            return Some(Handle::Middle);
        }
        if self.tool != Tool::Rotate {
            let inside = |index: usize| {
                let corners = plane_corners(camera, middle, axes, length, index)?;
                let on_screen = corners.map(|corner| camera.project(corner));
                inside_quad(point, on_screen.into_iter().collect::<Option<Vec<_>>>()?.try_into().ok()?).then_some(())
            };
            if let Some(index) = (0..3).find(|&index| inside(index).is_some()) {
                return Some(Handle::Plane(index));
            }
        }
        let mut best: Option<(f32, Handle)> = None;
        for (index, axis) in axes.iter().enumerate() {
            let near = if self.tool == Tool::Rotate {
                ring(camera, middle, *axis, length * AXIS_RING)
                    .windows(2)
                    .map(|w| to_segment(point, w[0], w[1]))
                    .fold(f32::MAX, f32::min)
            } else {
                let Some(tip) = camera.project(middle + *axis * length) else { continue };
                to_segment(point, centre, tip)
            };
            if near <= REACH && best.is_none_or(|(d, _)| near < d) {
                best = Some((near, Handle::Axis(index)));
            }
        }
        if best.is_none() && self.tool == Tool::Rotate {
            let from_centre = distance(point, centre);
            if (from_centre - VIEW_RING).abs() <= REACH {
                return Some(Handle::View);
            }
            if from_centre < HANDLE_LENGTH * AXIS_RING - REACH {
                return Some(Handle::Free);
            }
        }
        best.map(|(_, handle)| handle)
    }

    /// Notes which handle the pointer is over, to draw it lit.
    pub fn hover(&mut self, handle: Option<Handle>) {
        if self.drag.is_none() {
            self.hovered = handle;
        }
    }

    /// Takes hold of a handle where the pointer pressed.
    pub fn begin(
        &mut self,
        world: &World,
        renderer: &Renderer,
        camera: &ViewCamera,
        entity: Entity,
        handle: Handle,
        point: [f32; 2],
    ) {
        let Some((middle, axes, length)) = self.frame(world, renderer, camera, entity) else { return };
        let Some(&start) = world.get::<Transform>(entity) else { return };
        let matrix = world.get::<WorldTransform>(entity).map_or_else(|| start.matrix(), |w| w.0);
        let parent =
            world.get::<Parent>(entity).and_then(|p| world.get::<WorldTransform>(p.0)).map_or(Mat4::IDENTITY, |w| w.0);
        let ray = camera.ray(point);
        let (along, grip) = match handle {
            Handle::Axis(index) if self.tool == Tool::Rotate => {
                (0.0, on_plane(ray, middle, axes[index]).unwrap_or(middle))
            }
            Handle::Axis(index) => (along_axis(ray, middle, axes[index]).unwrap_or(0.0), middle),
            Handle::Plane(index) => (0.0, on_plane(ray, middle, axes[index]).unwrap_or(middle)),
            Handle::Middle | Handle::View | Handle::Free => {
                (0.0, on_plane(ray, middle, camera.forward).unwrap_or(middle))
            }
        };
        let outward = match handle {
            Handle::Plane(index) => plane_corners(camera, middle, axes, length, index)
                .map_or(Vec3::ZERO, |corners| ((corners[0] + corners[2]) * 0.5 - middle).normalize_or_zero()),
            _ => Vec3::ZERO,
        };
        self.drag = Some(Drag {
            entity,
            handle,
            start,
            middle,
            axes,
            along,
            grip,
            pointer: point,
            length,
            world_rotation: matrix.to_scale_rotation_translation().1,
            parent,
            view: [camera.right, camera.up, camera.forward],
            outward,
        });
        self.hovered = Some(handle);
    }

    /// Moves the held handle to a point, changing the Obj's Transform.
    /// `snap` is whether Ctrl is held.
    pub fn drag_to(&mut self, world: &mut World, camera: &ViewCamera, point: [f32; 2], snap: bool) {
        let Some(drag) = self.drag else { return };
        let ray = camera.ray(point);
        let mut transform = drag.start;
        let to_local = |world_position: Vec3| drag.parent.inverse().transform_point3(world_position);
        let start_world = drag.parent.transform_point3(drag.start.position);
        match (self.tool, drag.handle) {
            (Tool::Move, Handle::Axis(index)) => {
                let Some(along) = along_axis(ray, drag.middle, drag.axes[index]) else { return };
                let shift = snapped(along - drag.along, if snap { self.snap } else { 0.0 });
                transform.position = to_local(start_world + drag.axes[index] * shift);
            }
            (Tool::Move, Handle::Middle) => {
                let Some(hit) = on_plane(ray, drag.middle, camera.forward) else { return };
                let shift = (hit - drag.grip).to_array().map(|v| snapped(v, if snap { self.snap } else { 0.0 }));
                transform.position = to_local(start_world + Vec3::from_array(shift));
            }
            (Tool::Move, Handle::Plane(index)) => {
                let Some(hit) = on_plane(ray, drag.middle, drag.axes[index]) else { return };
                let shift = hit - drag.grip;
                let step = if snap { self.snap } else { 0.0 };
                let along = |axis: Vec3| axis * snapped(shift.dot(axis), step);
                let (a, b) = (drag.axes[(index + 1) % 3], drag.axes[(index + 2) % 3]);
                transform.position = to_local(start_world + along(a) + along(b));
            }
            (Tool::Rotate, Handle::Axis(index)) => {
                let Some(turn) = turn_on_plane(&drag, ray, drag.axes[index], snap) else { return };
                transform.rotation = drag.turned(turn);
            }
            (Tool::Rotate, Handle::View) => {
                let Some(turn) = turn_on_plane(&drag, ray, drag.view[2], snap) else { return };
                transform.rotation = drag.turned(turn);
            }
            (Tool::Rotate, Handle::Free) => {
                // The near side of the Obj follows the pointer, as a ball turned under the hand.
                let [right, up, forward] = drag.view;
                let moved = right * (point[0] - drag.pointer[0]) - up * (point[1] - drag.pointer[1]);
                let Some(axis) = (-forward).cross(moved).try_normalize() else { return };
                transform.rotation = drag.turned(Quat::from_axis_angle(axis, moved.length() * FREE_TURN));
            }
            (Tool::Scale, Handle::Axis(index)) => {
                let Some(along) = along_axis(ray, drag.middle, drag.axes[index]) else { return };
                let factor = 1.0 + (along - drag.along) / drag.length.max(1e-4);
                let mut scale = drag.start.scale.to_array();
                scale[index] = scaled(scale[index], factor, snap);
                transform.scale = Vec3::from_array(scale);
            }
            (Tool::Scale, Handle::Plane(index)) => {
                let Some(hit) = on_plane(ray, drag.middle, drag.axes[index]) else { return };
                let factor = 1.0 + (hit - drag.grip).dot(drag.outward) / drag.length.max(1e-4);
                let mut scale = drag.start.scale.to_array();
                for axis in [(index + 1) % 3, (index + 2) % 3] {
                    scale[axis] = scaled(scale[axis], factor, snap);
                }
                transform.scale = Vec3::from_array(scale);
            }
            (Tool::Scale, Handle::Middle) => {
                let factor = 1.0 + (point[0] - drag.pointer[0]) / 100.0;
                transform.scale = Vec3::from_array(drag.start.scale.to_array().map(|s| scaled(s, factor, snap)));
            }
            _ => return,
        }
        if let Some(current) = world.get_mut::<Transform>(drag.entity) {
            *current = transform;
        }
    }

    /// Lets go of the handle, returning the Obj and its Transform from
    /// before the drag, for undo.
    pub fn end(&mut self) -> Option<(Entity, Transform)> {
        self.drag.take().map(|drag| (drag.entity, drag.start))
    }

    /// The handles for the selected Obj, as shapes in the view's own
    /// coordinates (from its top left).
    pub fn shapes(
        &self,
        world: &World,
        renderer: &Renderer,
        camera: &ViewCamera,
        entity: Option<Entity>,
    ) -> Vec<Shape> {
        let Some(entity) = entity.filter(|_| self.tool != Tool::Select) else { return Vec::new() };
        let Some((middle, axes, length)) = self.frame(world, renderer, camera, entity) else { return Vec::new() };
        let Some(centre) = camera.project(middle) else { return Vec::new() };
        let local = |p: [f32; 2]| [p[0] - camera.rect[0], p[1] - camera.rect[1]];
        let colors = [0xeb5757, 0x4cb782, 0x5e8bf0].map(Color::hex);
        let lit = Color::hex(0xf2c94c);
        let color_of = |handle: Handle, base: Color| if self.hovered == Some(handle) { lit } else { base };
        let mut shapes = Vec::new();
        if self.tool == Tool::Rotate {
            // A faint ball inside the rings, which turns freely, then the ring around the view.
            let free = self.hovered == Some(Handle::Free);
            shapes.push(Shape::Circle {
                center: local(centre),
                radius: HANDLE_LENGTH * AXIS_RING,
                fill: Color::rgba(255, 255, 255, if free { 34 } else { 12 }),
                border: Some((1.0, Color::rgba(255, 255, 255, if free { 110 } else { 50 }))),
            });
            let view = color_of(Handle::View, Color::rgba(214, 217, 222, 230));
            let around: Vec<[f32; 2]> = (0..=RING_SEGMENTS)
                .map(|i| {
                    let angle = i as f32 / RING_SEGMENTS as f32 * std::f32::consts::TAU;
                    local([centre[0] + angle.cos() * VIEW_RING, centre[1] + angle.sin() * VIEW_RING])
                })
                .collect();
            for pair in around.windows(2) {
                shapes.push(Shape::Line { from: pair[0], to: pair[1], width: 1.5, color: view });
            }
        } else {
            for (index, color) in colors.iter().enumerate() {
                let Some(corners) = plane_corners(camera, middle, axes, length, index) else { continue };
                let Some(on_screen) =
                    corners.map(|c| camera.project(c).map(local)).into_iter().collect::<Option<Vec<_>>>()
                else {
                    continue;
                };
                shapes.extend(square(&on_screen, color_of(Handle::Plane(index), *color)));
            }
        }
        for (index, axis) in axes.iter().enumerate() {
            let color = color_of(Handle::Axis(index), colors[index]);
            if self.tool == Tool::Rotate {
                for pair in ring(camera, middle, *axis, length * AXIS_RING).windows(2) {
                    shapes.push(Shape::Line { from: local(pair[0]), to: local(pair[1]), width: 2.0, color });
                }
                continue;
            }
            let Some(tip) = camera.project(middle + *axis * length) else { continue };
            let (centre, tip) = (Vec2::from(local(centre)), Vec2::from(local(tip)));
            let Some(along) = (tip - centre).try_normalize() else { continue };
            // Move ends in an arrowhead and scale in a block, as in Unity; the
            // shaft stops where the end starts, so the end's shape stays clean.
            let end = if self.tool == Tool::Scale { BLOCK } else { ARROW_HEAD };
            let shaft_end = tip - along * end.min((tip - centre).length());
            shapes.push(Shape::Line { from: centre.into(), to: shaft_end.into(), width: 2.5, color });
            shapes.extend(if self.tool == Tool::Scale {
                block(tip, along, color)
            } else {
                arrow_head(tip, along, color)
            });
        }
        if self.tool != Tool::Rotate {
            let fill = color_of(Handle::Middle, Color::rgba(236, 237, 237, 220));
            shapes.push(Shape::Circle {
                center: local(centre),
                radius: 6.0,
                fill,
                border: Some((1.0, Color::rgba(12, 14, 16, 200))),
            });
        }
        shapes
    }
}

/// An arrowhead pointing along `along` with its point at `tip`, filled with
/// lines across it that narrow to the point.
fn arrow_head(tip: Vec2, along: Vec2, color: Color) -> Vec<Shape> {
    let across = along.perp();
    (0..=ARROW_HEAD as usize / 2)
        .map(|step| {
            let t = step as f32 / (ARROW_HEAD * 0.5);
            let at = tip - along * ARROW_HEAD * (1.0 - t);
            let half = ARROW_HEAD * 0.4 * (1.0 - t);
            Shape::Line { from: (at - across * half).into(), to: (at + across * half).into(), width: 2.0, color }
        })
        .collect()
}

/// A two-axis square from its corners on screen, in order: filled faintly
/// in its colour, with lines across it 2 pixels apart, and outlined.
fn square(corners: &[[f32; 2]], color: Color) -> Vec<Shape> {
    let [c0, c1, c2, c3] = [0, 1, 2, 3].map(|i| Vec2::from(corners[i]));
    let fill = color.with_alpha(105);
    let rows = ((c1 - c0).length().max((c2 - c3).length()) / 2.0).ceil().max(1.0) as usize;
    let mut shapes: Vec<Shape> = (0..=rows)
        .map(|row| {
            let t = row as f32 / rows as f32;
            Shape::Line { from: c0.lerp(c1, t).into(), to: c3.lerp(c2, t).into(), width: 2.0, color: fill }
        })
        .collect();
    for (from, to) in [(c0, c1), (c1, c2), (c2, c3), (c3, c0)] {
        shapes.push(Shape::Line { from: from.into(), to: to.into(), width: 1.5, color });
    }
    shapes
}

/// A square block centred a little short of `tip`, along `along`.
fn block(tip: Vec2, along: Vec2, color: Color) -> Vec<Shape> {
    let (across, half) = (along.perp(), BLOCK * 0.5);
    let middle = tip - along * half;
    (0..=BLOCK as usize / 2)
        .map(|step| {
            let at = middle + along * (step as f32 * 2.0 - half + 1.0);
            let (from, to) = (at - across * (half - 1.0), at + across * (half - 1.0));
            Shape::Line { from: from.into(), to: to.into(), width: 2.0, color }
        })
        .collect()
}

/// The turn around `axis` that brings where the drag took hold on its plane
/// round to where the pointer's ray meets it now, in 15 degree steps when
/// snapping.
fn turn_on_plane(drag: &Drag, ray: (Vec3, Vec3), axis: Vec3, snap: bool) -> Option<Quat> {
    let hit = on_plane(ray, drag.middle, axis)?;
    let (from, to) = (drag.grip - drag.middle, hit - drag.middle);
    let angle = axis.dot(from.cross(to)).atan2(from.dot(to));
    let angle = if snap { snapped(angle, 15f32.to_radians()) } else { angle };
    Some(Quat::from_axis_angle(axis, angle))
}

/// The corners, in the world, of the square across axis `index`: between
/// the other two axes, on the side of each that faces the camera. None when
/// the square is seen so edge on that it could not be told apart.
fn plane_corners(camera: &ViewCamera, middle: Vec3, axes: [Vec3; 3], length: f32, index: usize) -> Option<[Vec3; 4]> {
    let looking = (middle - camera.position).normalize_or(camera.forward);
    if axes[index].dot(looking).abs() < PLANE_EDGE {
        return None;
    }
    let toward = camera.position - middle;
    let facing = |axis: Vec3| if axis.dot(toward) >= 0.0 { axis } else { -axis } * length;
    let (u, v) = (facing(axes[(index + 1) % 3]), facing(axes[(index + 2) % 3]));
    let (near, far) = (PLANE_NEAR, PLANE_FAR);
    Some([middle + (u + v) * near, middle + u * far + v * near, middle + (u + v) * far, middle + u * near + v * far])
}

/// Whether a point on screen is inside a quadrilateral drawn in order, as
/// the projected square of a handle is: on the same side of every edge.
fn inside_quad(point: [f32; 2], corners: [[f32; 2]; 4]) -> bool {
    let side = |a: [f32; 2], b: [f32; 2]| (b[0] - a[0]) * (point[1] - a[1]) - (b[1] - a[1]) * (point[0] - a[0]);
    let sides: Vec<f32> = (0..4).map(|i| side(corners[i], corners[(i + 1) % 4])).collect();
    sides.iter().all(|&s| s >= 0.0) || sides.iter().all(|&s| s <= 0.0)
}

/// A circle in the world around `middle` in the plane across `axis`, as
/// points on screen; points behind the camera are left out.
pub(crate) fn ring(camera: &ViewCamera, middle: Vec3, axis: Vec3, radius: f32) -> Vec<[f32; 2]> {
    let across = axis.any_orthonormal_vector();
    let other = axis.cross(across);
    (0..=RING_SEGMENTS)
        .filter_map(|i| {
            let angle = i as f32 / RING_SEGMENTS as f32 * std::f32::consts::TAU;
            camera.project(middle + (across * angle.cos() + other * angle.sin()) * radius)
        })
        .collect()
}

fn distance(a: [f32; 2], b: [f32; 2]) -> f32 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

/// How far a point is from a segment on screen.
pub(crate) fn to_segment(point: [f32; 2], from: [f32; 2], to: [f32; 2]) -> f32 {
    let (dx, dy) = (to[0] - from[0], to[1] - from[1]);
    let length = dx * dx + dy * dy;
    let t = if length < 1e-6 {
        0.0
    } else {
        (((point[0] - from[0]) * dx + (point[1] - from[1]) * dy) / length).clamp(0.0, 1.0)
    };
    distance(point, [from[0] + dx * t, from[1] + dy * t])
}

/// A value rounded to a step; a step of 0 leaves it as it is.
pub(crate) fn snapped(value: f32, step: f32) -> f32 {
    if step > 0.0 { (value / step).round() * step } else { value }
}

/// A scale grown by a factor, kept above nothing, and to tenths when snapping.
fn scaled(scale: f32, factor: f32, snap: bool) -> f32 {
    let value = (scale * factor).max(0.001);
    if snap { snapped(value, 0.1).max(0.1) } else { value }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn camera() -> (World, ViewCamera) {
        let mut world = World::new();
        let transform = Transform::new(Vec3::new(0.0, 0.0, -10.0), Quat::IDENTITY, Vec3::ONE);
        let entity = world.spawn((Camera::default(), transform));
        let view = ViewCamera::new(&world, entity, [100.0, 50.0, 800.0, 600.0]).unwrap();
        (world, view)
    }

    #[test]
    fn rays_and_projection_agree_with_the_screen() {
        let (_, view) = camera();
        let centre = view.project(Vec3::ZERO).unwrap();
        assert!((centre[0] - 500.0).abs() < 0.01 && (centre[1] - 350.0).abs() < 0.01, "{centre:?}");
        let above = view.project(Vec3::new(0.0, 1.0, 0.0)).unwrap();
        assert!(above[1] < centre[1], "up in the world is up on screen");
        let right = view.project(Vec3::new(1.0, 0.0, 0.0)).unwrap();
        assert!(right[0] > centre[0], "and right is right");
        assert!(view.project(Vec3::new(0.0, 0.0, -20.0)).is_none(), "behind the camera is nowhere");

        let (origin, direction) = view.ray(above);
        let hit = origin + direction * ((0.0 - origin.z) / direction.z);
        assert!(
            hit.distance(Vec3::new(0.0, 1.0, 0.0)) < 1e-3,
            "a ray through a projected point passes through it: {hit}"
        );
    }

    #[test]
    fn rays_enter_boxes_and_meet_axes() {
        let box_at = enter_box(Vec3::new(0.0, 0.0, -10.0), Vec3::Z, Vec3::splat(-1.0), Vec3::ONE);
        assert_eq!(box_at, Some(9.0));
        assert_eq!(enter_box(Vec3::new(5.0, 0.0, -10.0), Vec3::Z, Vec3::splat(-1.0), Vec3::ONE), None);
        let flat =
            enter_box(Vec3::new(0.0, 5.0, 0.0), Vec3::NEG_Y, Vec3::new(-15.0, 0.0, -15.0), Vec3::new(15.0, 0.0, 15.0));
        assert_eq!(flat, Some(5.0), "a plane has a flat box, and is still hit");

        let ray = (Vec3::new(2.0, 0.0, -10.0), Vec3::Z);
        assert!((along_axis(ray, Vec3::ZERO, Vec3::X).unwrap() - 2.0).abs() < 1e-5);
        assert!(along_axis(ray, Vec3::ZERO, Vec3::Z).is_none(), "a ray along the axis cannot pick a place on it");
    }

    #[test]
    fn dragging_the_x_arrow_moves_along_x_and_snaps() {
        let (mut world, view) = camera();
        let crate_ = world.spawn((Transform::IDENTITY,));
        let mut tools = SceneTools::new();
        tools.tool = Tool::Move;
        let renderer_free = |tools: &mut SceneTools, world: &mut World| {
            // Without a renderer, the handles' frame is built by hand here.
            let length = view.world_size(Vec3::ZERO, HANDLE_LENGTH);
            tools.drag = Some(Drag {
                entity: crate_,
                handle: Handle::Axis(0),
                start: Transform::IDENTITY,
                middle: Vec3::ZERO,
                axes: [Vec3::X, Vec3::Y, Vec3::Z],
                along: along_axis(view.ray(view.project(Vec3::ZERO).unwrap()), Vec3::ZERO, Vec3::X).unwrap(),
                grip: Vec3::ZERO,
                pointer: [0.0; 2],
                length,
                world_rotation: Quat::IDENTITY,
                parent: Mat4::IDENTITY,
                view: [view.right, view.up, view.forward],
                outward: Vec3::ZERO,
            });
            let _ = world;
        };
        renderer_free(&mut tools, &mut world);
        let to = view.project(Vec3::new(1.3, 0.4, 0.0)).unwrap();
        tools.drag_to(&mut world, &view, to, false);
        let moved = world.get::<Transform>(crate_).unwrap().position;
        // The nearest place on the axis to the pointer's ray, which perspective moves a little off 1.3.
        assert!((moved.x - 1.3).abs() < 0.01 && moved.y.abs() < 1e-6 && moved.z.abs() < 1e-6, "only along x: {moved}");
        tools.drag_to(&mut world, &view, to, true);
        assert!((world.get::<Transform>(crate_).unwrap().position.x - 1.25).abs() < 1e-4, "snapped to 0.25");
        assert_eq!(tools.end().map(|(e, t)| (e, t.position)), Some((crate_, Vec3::ZERO)), "the start, for undo");
    }

    /// A drag of `handle` on an Obj at the origin with the world's axes,
    /// taken at `grip`, for the tests: the frame `begin` would build.
    fn drag_of(view: &ViewCamera, entity: Entity, handle: Handle, grip: Vec3, outward: Vec3) -> Drag {
        Drag {
            entity,
            handle,
            start: Transform::IDENTITY,
            middle: Vec3::ZERO,
            axes: [Vec3::X, Vec3::Y, Vec3::Z],
            along: 0.0,
            grip,
            pointer: view.project(grip).unwrap(),
            length: view.world_size(Vec3::ZERO, HANDLE_LENGTH),
            world_rotation: Quat::IDENTITY,
            parent: Mat4::IDENTITY,
            view: [view.right, view.up, view.forward],
            outward,
        }
    }

    #[test]
    fn a_square_moves_along_its_two_axes_and_not_the_third() {
        let (mut world, view) = camera();
        let crate_ = world.spawn((Transform::IDENTITY,));
        let mut tools = SceneTools { tool: Tool::Move, ..SceneTools::new() };
        tools.drag = Some(drag_of(&view, crate_, Handle::Plane(2), Vec3::ZERO, Vec3::ZERO));
        tools.drag_to(&mut world, &view, view.project(Vec3::new(1.0, 0.5, 0.0)).unwrap(), false);
        let moved = world.get::<Transform>(crate_).unwrap().position;
        assert!(moved.distance(Vec3::new(1.0, 0.5, 0.0)) < 1e-3, "along x and y, on the square's plane: {moved}");
        tools.drag_to(&mut world, &view, view.project(Vec3::new(1.1, 0.4, 0.0)).unwrap(), true);
        let snapped = world.get::<Transform>(crate_).unwrap().position;
        assert!(snapped.distance(Vec3::new(1.0, 0.5, 0.0)) < 1e-4, "each axis snapped to 0.25: {snapped}");
    }

    #[test]
    fn a_square_is_found_inside_it_and_hides_edge_on() {
        let (_, view) = camera();
        let tools = SceneTools { tool: Tool::Move, ..SceneTools::new() };
        let length = view.world_size(Vec3::ZERO, HANDLE_LENGTH);
        let axes = [Vec3::X, Vec3::Y, Vec3::Z];
        let corners = plane_corners(&view, Vec3::ZERO, axes, length, 2).expect("the square facing the camera");
        let inside = view.project((corners[0] + corners[2]) * 0.5).unwrap();
        assert_eq!(tools.handle_in(&view, (Vec3::ZERO, axes, length), inside), Some(Handle::Plane(2)));
        assert!(plane_corners(&view, Vec3::ZERO, axes, length, 0).is_none(), "the square across x is edge on");
        let outside = view.project(corners[2] + (corners[2] - corners[0])).unwrap();
        assert_ne!(tools.handle_in(&view, (Vec3::ZERO, axes, length), outside), Some(Handle::Plane(2)));
    }

    #[test]
    fn the_view_ring_turns_around_the_way_the_view_looks() {
        let (mut world, view) = camera();
        let crate_ = world.spawn((Transform::IDENTITY,));
        let mut tools = SceneTools { tool: Tool::Rotate, ..SceneTools::new() };
        tools.drag = Some(drag_of(&view, crate_, Handle::View, Vec3::X, Vec3::ZERO));
        tools.drag_to(&mut world, &view, view.project(Vec3::Y).unwrap(), false);
        let turned = world.get::<Transform>(crate_).unwrap().rotation;
        assert!((turned * Vec3::X).distance(Vec3::Y) < 1e-3, "a quarter turn around the view: {turned}");
        assert!((turned * Vec3::Z).distance(Vec3::Z) < 1e-3, "and the view's own direction stays");
    }

    #[test]
    fn turning_freely_brings_the_near_side_after_the_pointer() {
        let (mut world, view) = camera();
        let crate_ = world.spawn((Transform::IDENTITY,));
        let mut tools = SceneTools { tool: Tool::Rotate, ..SceneTools::new() };
        tools.drag = Some(drag_of(&view, crate_, Handle::Free, Vec3::ZERO, Vec3::ZERO));
        let start = view.project(Vec3::ZERO).unwrap();
        tools.drag_to(&mut world, &view, [start[0] + 50.0, start[1]], false);
        let turned = world.get::<Transform>(crate_).unwrap().rotation;
        let near = turned * Vec3::NEG_Z;
        assert!(near.x > 0.4 && near.y.abs() < 1e-4, "the side facing the camera went right: {near}");
        tools.drag_to(&mut world, &view, [start[0], start[1] - 50.0], false);
        let near = world.get::<Transform>(crate_).unwrap().rotation * Vec3::NEG_Z;
        assert!(near.y > 0.4 && near.x.abs() < 1e-4, "and up, with the pointer: {near}");
    }

    #[test]
    fn a_scale_square_scales_its_two_axes_together() {
        let (mut world, view) = camera();
        let crate_ = world.spawn((Transform::IDENTITY,));
        let mut tools = SceneTools { tool: Tool::Scale, ..SceneTools::new() };
        let outward = Vec3::new(1.0, 1.0, 0.0).normalize();
        let drag = drag_of(&view, crate_, Handle::Plane(2), Vec3::new(0.3, 0.3, 0.0), outward);
        let length = drag.length;
        tools.drag = Some(drag);
        let to = view.project(Vec3::new(0.3, 0.3, 0.0) + outward * length * 0.5).unwrap();
        tools.drag_to(&mut world, &view, to, false);
        let scale = world.get::<Transform>(crate_).unwrap().scale;
        assert!((scale.x - 1.5).abs() < 1e-3 && (scale.y - 1.5).abs() < 1e-3, "x and y grow together: {scale}");
        assert_eq!(scale.z, 1.0, "and z stays");
    }
}
