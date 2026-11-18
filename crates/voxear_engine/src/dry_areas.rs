//! Dry Areas for scripts (`DryArea` in Voxear.Core): their shape and size,
//! and the water standing in them, their bilge, which a script can raise a
//! little each frame as a ship's hold floods. Nothing is made again when it
//! changes: the renderer and the water scripts and floating bodies ask about
//! read the Dry Area afresh each frame.

use voxear_core::math::{Mat4, Vec3};
use voxear_ecs::{Entity, World};
use voxear_render::{DryArea, DryShape, MeshRenderer};

use crate::mesh_boxes::MeshBoxes;

/// Sets what answers `DryArea` in scripts.
pub(crate) fn register() {
    voxear_scripting::set_kind_values(voxear_scripting::ComponentKind::DryArea, values);
}

// What `values` reads or writes, as `DryArea` in Voxear.Core numbers them.
const HAS: i32 = 0;
const ADD: i32 = 1;
const REMOVE: i32 = 2;
const SHAPE: i32 = 3;
const SIZE: i32 = 4;
const BILGE: i32 = 5;
const BILGE_FILL: i32 = 6;
const BILGE_ON: i32 = 7;
const INSIDE_HEIGHT: i32 = 8;

/// Reads value `what` of `entity`'s Dry Area, or with `written`, writes it:
/// `None` when it has none, or is gone.
fn values(world: &mut World, entity: Entity, what: i32, written: Option<[f32; 4]>) -> Option<[f32; 4]> {
    if !world.is_alive(entity) {
        return None;
    }
    let number = |n: f32| [n, 0.0, 0.0, 0.0];
    match (what, written) {
        (HAS, None) => return Some(number(f32::from(u8::from(world.has::<DryArea>(entity))))),
        (ADD, Some(_)) => {
            if !world.has::<DryArea>(entity) {
                world.insert(entity, DryArea::default());
            }
            return Some([0.0; 4]);
        }
        (REMOVE, Some(_)) => return world.remove::<DryArea>(entity).map(|_| [0.0; 4]),
        _ => {}
    }
    keep_mesh_box(world, entity);
    let model = placed(world, entity)?;
    let area = world.get_mut::<DryArea>(entity)?;
    let tall = area.inside_height(model);
    match written {
        None => Some(match what {
            SHAPE => number(area.shape as u8 as f32),
            SIZE => Vec3::from(area.size).extend(0.0).to_array(),
            BILGE => number(area.bilge),
            BILGE_FILL => number(if tall > 0.0 { (area.bilge / tall).clamp(0.0, 1.0) } else { 0.0 }),
            BILGE_ON => number(f32::from(u8::from(area.bilge_on))),
            INSIDE_HEIGHT => number(tall),
            _ => return None,
        }),
        Some([x, y, z, _]) => {
            match what {
                SHAPE => {
                    area.shape = *DryShape::ALL.get(x as usize)?;
                    keep_mesh_box(world, entity);
                }
                SIZE => area.size = [x, y, z].map(|side| side.clamp(0.01, 10_000.0)),
                BILGE => area.bilge = x.clamp(0.0, 10_000.0),
                BILGE_FILL => area.bilge = x.clamp(0.0, 1.0) * tall,
                BILGE_ON => area.bilge_on = x != 0.0,
                _ => return None,
            }
            Some([0.0; 4])
        }
    }
}

/// How a Dry Area's inside is placed for how tall it stands: a Mesh's by its
/// Obj's whole matrix, scale and all; the other shapes keep their size
/// however their Obj is scaled.
fn placed(world: &World, entity: Entity) -> Option<Mat4> {
    Some(match world.get::<DryArea>(entity)?.shape {
        DryShape::Mesh => voxear_scene::world_matrix(world, entity),
        _ => Mat4::IDENTITY,
    })
}

/// Gives a Mesh Dry Area the box round its mesh, which the water scripts and
/// floating bodies ask about takes as its inside, once the renderer knows it:
/// so one a script made, or turned to a Mesh, has one as a loaded one has.
fn keep_mesh_box(world: &mut World, entity: Entity) {
    let Some(area) = world.get::<DryArea>(entity).filter(|area| area.shape == DryShape::Mesh && area.bounds.is_none())
    else {
        return;
    };
    let mesh = area.mesh.or(world.get::<MeshRenderer>(entity).map(|drawn| drawn.mesh));
    let Some((least, most)) = mesh.and_then(|mesh| world.resource::<MeshBoxes>()?.of(mesh)) else { return };
    if let Some(area) = world.get_mut::<DryArea>(entity) {
        area.bounds = Some([least, most]);
    }
}
