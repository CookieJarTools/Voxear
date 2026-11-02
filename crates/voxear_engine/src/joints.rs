//! A Joint as a scene or prefab file writes it, the Obj it is connected to
//! named by its place in the same file, as a LOD Group's Objs are:
//!
//! ```toml
//! [obj.joint]
//! type = "hinge"         # fixed, hinge, ball or distance
//! connected = 3          # the Obj it is held to; left out, the world
//! anchor = [-0.5, 0.0, 0.0]
//! axis = [0.0, 1.0, 0.0]
//! limits = [0.0, 90.0]   # degrees for a hinge, metres for a distance
//! ```

use anyhow::Result;
use serde::{Deserialize, Serialize};
use voxear_ecs::Entity;

use crate::{
    lod_groups::FileRef,
    physics::{Joint, JointType},
    prefab::ObjPath,
};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JointData {
    #[serde(rename = "type", default, skip_serializing_if = "is_fixed")]
    pub kind: JointType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connected: Option<FileRef>,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub anchor: [f32; 3],
    #[serde(default = "up", skip_serializing_if = "is_up")]
    pub axis: [f32; 3],
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limits: Option<[f32; 2]>,
}

impl JointData {
    /// A joint as a file saves it, its connected Obj where `path` says it
    /// is in that file. One connected to an Obj not being saved is saved
    /// held to the world, like a missing reference in Unity.
    pub fn of(joint: &Joint, path: &dyn Fn(Entity) -> Option<ObjPath>) -> Self {
        Self {
            kind: joint.kind,
            connected: joint.connected.and_then(path).map(|path| FileRef::new(&path)),
            anchor: joint.anchor,
            axis: joint.axis,
            limits: joint.limits,
        }
    }

    /// The component, its connected Obj found by `lookup`.
    pub fn bind(&self, lookup: &dyn Fn(&ObjPath) -> Option<Entity>) -> Joint {
        let connected = self.connected.as_ref().and_then(|obj| obj.path().ok()).and_then(|path| lookup(&path));
        Joint { kind: self.kind, connected, anchor: self.anchor, axis: self.axis, limits: self.limits }
    }

    /// The Obj it is connected to, when it names one.
    pub fn refs(&self) -> Result<Vec<ObjPath>> {
        self.connected.iter().map(FileRef::path).collect()
    }

    /// Rewrites the Obj it is connected to with `f`, as a prefab's Objs
    /// are named from the instance in the file that places it.
    pub fn map_refs(&mut self, f: &dyn Fn(&ObjPath) -> ObjPath) -> Result<()> {
        if let Some(obj) = &mut self.connected {
            *obj = FileRef::new(&f(&obj.path()?));
        }
        Ok(())
    }

    /// The whole joint as an override writes it. Every value is written,
    /// since an override merges into the prefab's and a value left out
    /// would keep the prefab's; a joint held to the world says so.
    pub fn whole(&self) -> Result<toml::Value> {
        let mut table = toml::Table::try_from(self)?;
        table.insert("type".into(), toml::Value::try_from(self.kind)?);
        table.insert("anchor".into(), toml::Value::try_from(self.anchor)?);
        table.insert("axis".into(), toml::Value::try_from(self.axis)?);
        Ok(toml::Value::Table(table))
    }
}

fn is_fixed(kind: &JointType) -> bool {
    *kind == JointType::Fixed
}

fn is_zero(value: &[f32; 3]) -> bool {
    *value == [0.0; 3]
}

fn up() -> [f32; 3] {
    [0.0, 1.0, 0.0]
}

fn is_up(value: &[f32; 3]) -> bool {
    *value == up()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_joint_names_its_connected_obj_by_its_place_in_the_file() {
        let text = "type = \"hinge\"\nconnected = 3\nlimits = [0.0, 90.0]\n";
        let data: JointData = toml::from_str(text).unwrap();
        assert_eq!((data.kind, data.axis), (JointType::Hinge, [0.0, 1.0, 0.0]));
        assert_eq!(data.refs().unwrap(), [ObjPath::new(3)]);
        let (lamp, other) = (Entity::from_bits(1 << 32 | 7), Entity::from_bits(1 << 32 | 8));
        let joint = data.bind(&|path| (*path == ObjPath::new(3)).then_some(lamp));
        assert_eq!(joint.connected, Some(lamp));
        let saved = JointData::of(&joint, &|entity| (entity == lamp).then(|| ObjPath::new(3)));
        assert_eq!(toml::to_string(&saved).unwrap(), text);
        let to_world = JointData::of(&Joint { connected: Some(other), ..joint }, &|_| None);
        assert_eq!(to_world.connected, None, "an Obj not saved with it leaves it held to the world");
    }
}
