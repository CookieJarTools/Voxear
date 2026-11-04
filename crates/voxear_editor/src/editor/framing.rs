//! Frame Selected, Align With View and Unpack Prefab, as Unity's F,
//! Ctrl+Shift+F and the Hierarchy's Unpack Prefab:
//!
//! - Framing glides the Scene view's camera to show the selection whole,
//!   keeping the way it looks: everything drawn in it and inside it, or a
//!   metre around an Obj that draws nothing.
//! - Aligning moves and turns each Obj selected to where the Scene view's
//!   camera is, as one step to undo.
//! - Unpacking makes a prefab instance's Objs Objs of the scene, which
//!   follow the prefab no more, as one step to undo.

use voxear_core::math::{Mat4, Vec3};
use voxear_ecs::Entity;
use voxear_engine::{
    prefab::{FromPrefab, PrefabInstance},
    prefab_overrides,
};
use voxear_render::{Camera, Renderer};
use voxear_scene::{Name, Parent, Transform};

use super::Editor;
use crate::{
    history::{Edit, Link, ObjState},
    props,
};

/// How long the camera takes to frame the selection, in seconds.
const GLIDE_SECONDS: f32 = 0.25;
/// Room around what is framed, as a share of its size.
const MARGIN: f32 = 1.15;

/// The Scene view's camera on its way from one place to another.
pub(super) struct Glide {
    from: Vec3,
    to: Vec3,
    done: f32,
}

impl Editor {
    /// Glides the Scene view's camera to show `objs` whole, looking the way
    /// it looks now.
    pub(super) fn frame(&mut self, objs: &[Entity], renderer: &Renderer) {
        let Some((centre, radius)) = bounds(&self.game, renderer, objs) else {
            return self.notify("Select an Obj to frame it");
        };
        let world = &self.game.engine.world;
        let Some(camera) = world.get::<Transform>(self.scene.camera) else { return };
        let fov = world.get::<Camera>(self.scene.camera).map_or(60.0, |lens| lens.fov_y_degrees);
        // Far enough that the sphere around it fits the view's height.
        let distance = radius * MARGIN / (fov.to_radians() / 2.0).sin();
        let to = centre - camera.rotation * Vec3::Z * distance;
        self.glide = Some(Glide { from: camera.position, to, done: 0.0 });
    }

    /// Moves the camera along its glide, which flying the camera ends.
    pub(super) fn update_glide(&mut self, delta: f32) {
        if self.fly().is_looking() {
            self.glide = None;
        }
        let Some(glide) = &mut self.glide else { return };
        glide.done = (glide.done + delta / GLIDE_SECONDS).min(1.0);
        let eased = 1.0 - (1.0 - glide.done).powi(3);
        let position = glide.from.lerp(glide.to, eased);
        let finished = glide.done >= 1.0;
        if let Some(camera) = self.game.engine.world.get_mut::<Transform>(self.scene.camera) {
            camera.position = position;
        }
        if finished {
            self.glide = None;
        }
    }

    /// Moves and turns each Obj selected to where the Scene view's camera
    /// is and the way it looks.
    pub(super) fn align_with_view(&mut self) {
        let world = &self.game.engine.world;
        let Some(view) = world.get::<Transform>(self.scene.camera).copied() else { return };
        let objs = self.selected_objs();
        if objs.is_empty() {
            return self.notify("Select an Obj to align it with the view");
        }
        let mut edits = Vec::new();
        for obj in objs {
            let world = &mut self.game.engine.world;
            let Some(before) = ObjState::of(world, obj) else { continue };
            let parent = world.get::<Parent>(obj).map_or(Mat4::IDENTITY, |p| voxear_scene::world_matrix(world, p.0));
            let local = parent.inverse() * Mat4::from_rotation_translation(view.rotation, view.position);
            let (_, rotation, position) = local.to_scale_rotation_translation();
            if let Some(transform) = world.get_mut::<Transform>(obj) {
                (transform.position, transform.rotation) = (position, rotation);
            }
            let Some(after) = ObjState::of(world, obj) else { continue };
            if before != after {
                edits.push(Edit::Changed { entity: obj, before, after });
            }
        }
        if !edits.is_empty() {
            self.history.record_step(Edit::Group(edits));
            self.unsaved = true;
        }
    }

    /// Makes the instance whose root is `root` Objs of the scene.
    pub(super) fn unpack_prefab(&mut self, root: Entity) {
        if self.is_playing() {
            return self.notify("Stop playing to unpack a prefab");
        }
        let world = &self.game.engine.world;
        let name = world.get::<Name>(root).map_or_else(String::new, |name| name.0.clone());
        let links: Vec<Link> = prefab_overrides::unpacked(world, root)
            .into_iter()
            .map(|(entity, from, instance)| {
                let before = (world.get::<FromPrefab>(entity).cloned(), world.get::<PrefabInstance>(entity).copied());
                Link { entity, before, after: (from, instance) }
            })
            .collect();
        for link in &links {
            link.apply(&mut self.game.engine.world, true);
        }
        self.history.record_step(Edit::Linked(links));
        self.unsaved = true;
        self.overrides_seen = None;
        self.select(Some(root));
        self.notify(&format!("Unpacked {name}"));
    }
}

/// The sphere around what `objs` and the Objs inside them draw, in the
/// world; around where they are when none draws anything.
fn bounds(game: &voxear_engine::Game, renderer: &Renderer, objs: &[Entity]) -> Option<(Vec3, f32)> {
    let world = &game.engine.world;
    let all: Vec<Entity> =
        objs.iter().flat_map(|&obj| std::iter::once(obj).chain(props::descendants(game, obj))).collect();
    let (mut low, mut high) = (Vec3::MAX, Vec3::MIN);
    for &entity in &all {
        let Some((min, max)) = super::selection::drawn_bounds(world, renderer, entity) else {
            continue;
        };
        let matrix = voxear_scene::world_matrix(world, entity);
        for corner in 0..8 {
            let pick = |bit: usize, a: f32, b: f32| if corner & bit == 0 { a } else { b };
            let local = Vec3::new(pick(1, min.x, max.x), pick(2, min.y, max.y), pick(4, min.z, max.z));
            let point = matrix.transform_point3(local);
            (low, high) = (low.min(point), high.max(point));
        }
    }
    if low.x > high.x {
        for &obj in objs {
            let point = voxear_scene::world_matrix(world, obj).w_axis.truncate();
            (low, high) = (low.min(point - Vec3::splat(0.5)), high.max(point + Vec3::splat(0.5)));
        }
    }
    (low.x <= high.x).then(|| ((low + high) / 2.0, ((high - low).length() / 2.0).max(0.1)))
}
