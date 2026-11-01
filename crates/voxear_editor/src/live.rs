//! A running game's C# component values, read from and written to the
//! component's own bytes in the world, where ScriptInfo said each field
//! lies (`FieldLayout`). In edit mode an Obj's components are only data, so
//! nothing here applies, and the Inspector shows and edits that data.

use voxear_core::math::{EulerRot, Quat};
use voxear_ecs::{ComponentId, Entity, World};
use voxear_engine::script_types::{FieldKind, FieldLayout, ScriptTypes};
use voxear_scene::{Behaviours, Name};

use crate::{fields::Step, props::Value};

/// Where a top-level field of a component the Obj has now lies.
fn slot(
    world: &World,
    types: &ScriptTypes,
    entity: Entity,
    component: &str,
    path: &[Step],
) -> Option<(ComponentId, FieldLayout, FieldKind)> {
    let [Step::Key(name)] = path else { return None };
    if !types.is_component(component) {
        return None;
    }
    let field = types.behaviour(component)?.field(name)?;
    let id = world.components().id_by_name(component)?;
    let layout = field.layout?;
    let fits = layout.offset + layout.size <= world.components().info(id).layout().size() as u32;
    (world.has_id(entity, id) && fits).then(|| (id, layout, field.kind.clone()))
}

/// Whether a behaviour of an Obj is running, as while the game plays, so its
/// fields are read from and written to its instance.
pub fn is_running(world: &World, entity: Entity, behaviour: &str) -> bool {
    world.get::<Behaviours>(entity).is_some_and(|running| running.type_names().any(|name| name == behaviour))
}

/// Whether a field is shown and edited from the running game.
pub fn is_live(world: &World, types: &ScriptTypes, entity: Entity, component: &str, path: &[Step]) -> bool {
    slot(world, types, entity, component, path).is_some()
}

/// The field's value in the running game.
pub fn get(world: &World, types: &ScriptTypes, entity: Entity, component: &str, path: &[Step]) -> Option<Value> {
    let (id, layout, kind) = slot(world, types, entity, component, path)?;
    let base = world.get_by_id(entity, id)?;
    let bytes = unsafe { std::slice::from_raw_parts(base.as_ptr().add(layout.offset as usize), layout.size as usize) };
    Some(decode(world, &kind, layout, bytes))
}

/// Writes the field's value into the running game, returning whether it
/// changed.
pub fn set(
    world: &mut World,
    types: &ScriptTypes,
    entity: Entity,
    component: &str,
    path: &[Step],
    value: &Value,
) -> bool {
    let Some((id, layout, kind)) = slot(world, types, entity, component, path) else { return false };
    if get(world, types, entity, component, path).as_ref() == Some(value) {
        return false;
    }
    let current = match world.get_by_id(entity, id) {
        Some(base) => {
            unsafe { std::slice::from_raw_parts(base.as_ptr().add(layout.offset as usize), layout.size as usize) }
                .to_vec()
        }
        None => return false,
    };
    let Some(bytes) = encode(&kind, layout, value, &current) else { return false };
    let Some(base) = world.get_mut_by_id(entity, id) else { return false };
    let target = unsafe { base.as_ptr().add(layout.offset as usize) };
    unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), target, bytes.len()) };
    true
}

fn decode(world: &World, kind: &FieldKind, layout: FieldLayout, bytes: &[u8]) -> Value {
    let floats = || bytes.as_chunks::<4>().0.iter().map(|b| f32::from_le_bytes(*b)).collect::<Vec<_>>();
    match kind {
        FieldKind::Number if layout.size == 8 => Value::Number(f64::from_le_bytes(eight(bytes)) as f32),
        FieldKind::Number => Value::Number(floats()[0]),
        FieldKind::Whole => Value::Whole(whole(bytes, layout.signed)),
        FieldKind::Bool => Value::Toggle(bytes[0] != 0),
        FieldKind::Enum { names } => {
            let index = usize::try_from(whole(bytes, layout.signed)).unwrap_or_default();
            Value::Choice(index.min(names.len().saturating_sub(1)), names.clone())
        }
        FieldKind::Vector { .. } => Value::Vector(floats()),
        FieldKind::Color => {
            let rgb = floats();
            Value::Color([rgb[0], rgb[1], rgb[2]])
        }
        FieldKind::Rotation => {
            let [x, y, z, w] = floats()[..4].try_into().unwrap_or([0.0, 0.0, 0.0, 1.0]);
            let (y, x, z) = Quat::from_xyzw(x, y, z, w).normalize().to_euler(EulerRot::YXZ);
            Value::Vector([x, y, z].iter().map(|a| a.to_degrees()).collect())
        }
        FieldKind::Obj => {
            let entity = Entity::from_bits(u64::from_le_bytes(eight(bytes)));
            Value::Obj(world.get::<Name>(entity).map_or_else(|| "None".into(), |name| name.0.clone()))
        }
        _ => Value::Fixed(format!("{} bytes", bytes.len())),
    }
}

/// The bytes for a value; `current` is what the field holds now, which
/// keeps a colour's alpha, since the Inspector picks only the rest.
fn encode(kind: &FieldKind, layout: FieldLayout, value: &Value, current: &[u8]) -> Option<Vec<u8>> {
    let size = layout.size as usize;
    let bytes = match (kind, value) {
        (FieldKind::Number, Value::Number(n)) if size == 8 => f64::from(*n).to_le_bytes().to_vec(),
        (FieldKind::Number, Value::Number(n)) => n.to_le_bytes().to_vec(),
        (FieldKind::Whole, Value::Whole(n)) => n.to_le_bytes()[..size].to_vec(),
        (FieldKind::Bool, Value::Toggle(b)) => vec![u8::from(*b)],
        (FieldKind::Enum { .. }, Value::Choice(index, _)) => (*index as i64).to_le_bytes()[..size].to_vec(),
        (FieldKind::Vector { .. }, Value::Vector(values)) => values.iter().flat_map(|v| v.to_le_bytes()).collect(),
        (FieldKind::Color, Value::Color(rgb)) => {
            let mut bytes: Vec<u8> = rgb.iter().flat_map(|v| v.to_le_bytes()).collect();
            bytes.extend_from_slice(current.get(12..16)?);
            bytes
        }
        (FieldKind::Rotation, Value::Vector(degrees)) => {
            let [x, y, z] = degrees[..3].try_into().ok()?;
            let q = Quat::from_euler(EulerRot::YXZ, f32::to_radians(y), f32::to_radians(x), f32::to_radians(z));
            q.to_array().iter().flat_map(|v| v.to_le_bytes()).collect()
        }
        _ => return None,
    };
    (bytes.len() == size).then_some(bytes)
}

fn eight(bytes: &[u8]) -> [u8; 8] {
    bytes[..8].try_into().unwrap_or_default()
}

/// A whole number of one to eight bytes, little-endian, widened to 64 bits.
fn whole(bytes: &[u8], signed: bool) -> i64 {
    let mut wide = [0u8; 8];
    wide[..bytes.len()].copy_from_slice(bytes);
    let value = i64::from_le_bytes(wide);
    let unused = 64 - 8 * bytes.len() as u32;
    match signed && unused > 0 {
        true => (value << unused) >> unused,
        false => value,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_colour_is_written_keeping_its_alpha() {
        let layout = FieldLayout { offset: 0, size: 16, signed: false };
        let current: Vec<u8> = [1.0f32, 0.5, 0.0, 0.25].iter().flat_map(|v| v.to_le_bytes()).collect();
        let world = World::new();
        assert_eq!(decode(&world, &FieldKind::Color, layout, &current), Value::Color([1.0, 0.5, 0.0]));
        let bytes = encode(&FieldKind::Color, layout, &Value::Color([0.0, 1.0, 0.0]), &current).unwrap();
        let floats: Vec<f32> = bytes.as_chunks::<4>().0.iter().map(|b| f32::from_le_bytes(*b)).collect();
        assert_eq!(floats, [0.0, 1.0, 0.0, 0.25]);
    }

    #[test]
    fn whole_numbers_widen_with_their_sign() {
        assert_eq!(whole(&(-3i16).to_le_bytes(), true), -3);
        assert_eq!(whole(&65533u16.to_le_bytes(), false), 65533);
        assert_eq!(whole(&(-1i64).to_le_bytes(), true), -1);
        assert_eq!(whole(&[7], false), 7);
    }
}
