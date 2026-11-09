//! The boxes round the meshes Objs draw, for scripts (`Obj.MeshBox` in
//! Voxear.Core). Only the renderer knows a mesh's box, once it has the
//! mesh, so the game copies the boxes into the world whenever the
//! renderer's change. A game with no window to draw in has none.

use std::collections::HashMap;

use voxear_core::math::Vec3;
use voxear_ecs::{Entity, World};
use voxear_render::{MeshHandle, MeshRenderer, Renderer};
use voxear_scripting::ComponentKind;

// What `values` reads, as `MeshBoxes` in Voxear.Core numbers them.
const HAS: i32 = 0;
const LEAST: i32 = 3;
const MOST: i32 = 4;

/// The renderer's meshes' boxes in their own space, as they were after
/// its `changes`th change to them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MeshBoxes {
    changes: u64,
    boxes: HashMap<MeshHandle, (Vec3, Vec3)>,
}

impl MeshBoxes {
    /// A mesh's box, its least and most corners, once the renderer has it.
    pub fn of(&self, mesh: MeshHandle) -> Option<(Vec3, Vec3)> {
        self.boxes.get(&mesh).copied()
    }
}

/// Copies the renderer's boxes into the world, when they changed since the
/// copy there was made.
pub fn refresh(world: &mut World, renderer: &Renderer) {
    let Some((changes, boxes)) = renderer.mesh_boxes() else { return };
    if world.resource::<MeshBoxes>().is_some_and(|kept| kept.changes == changes) {
        return;
    }
    world.insert_resource(MeshBoxes { changes, boxes: boxes.clone() });
}

pub(crate) fn register() {
    voxear_scripting::set_kind_values(ComponentKind::MeshRenderer, values);
}

fn values(world: &mut World, entity: Entity, what: i32, written: Option<[f32; 4]>) -> Option<[f32; 4]> {
    if written.is_some() || !world.is_alive(entity) {
        return None;
    }
    let mesh = world.get::<MeshRenderer>(entity).map(|renderer| renderer.mesh);
    if what == HAS {
        return Some([f32::from(u8::from(mesh.is_some())), 0.0, 0.0, 0.0]);
    }
    let (least, most) = world.resource::<MeshBoxes>()?.of(mesh?)?;
    match what {
        LEAST => Some(least.extend(0.0).to_array()),
        MOST => Some(most.extend(0.0).to_array()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use voxear_render::MaterialHandle;

    use super::*;

    #[test]
    fn an_objs_mesh_box_is_read_once_the_renderer_has_it() {
        let mut world = World::new();
        let drawn = world.spawn((MeshRenderer::new(MeshHandle(3), MaterialHandle(0)),));
        let bare = world.spawn(());
        assert_eq!(values(&mut world, drawn, HAS, None), Some([1.0, 0.0, 0.0, 0.0]));
        assert_eq!(values(&mut world, bare, HAS, None), Some([0.0; 4]));
        assert_eq!(values(&mut world, drawn, LEAST, None), None, "no box before the renderer has the mesh");

        let boxes = HashMap::from([(MeshHandle(3), (Vec3::splat(-0.5), Vec3::new(0.5, 1.0, 0.5)))]);
        world.insert_resource(MeshBoxes { changes: 1, boxes });
        assert_eq!(values(&mut world, drawn, LEAST, None), Some([-0.5, -0.5, -0.5, 0.0]));
        assert_eq!(values(&mut world, drawn, MOST, None), Some([0.5, 1.0, 0.5, 0.0]));
        assert_eq!(values(&mut world, drawn, MOST, Some([0.0; 4])), None, "a box is only read");
    }
}
