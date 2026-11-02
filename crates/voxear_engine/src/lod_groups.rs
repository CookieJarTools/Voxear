//! LOD Groups in scene and prefab files, as `[obj.lod_group]`. Each level
//! names its Obj the way other references in a file do: by id, or by path
//! inside a prefab instance (see [`ObjPath`]).
//!
//! ```toml
//! [obj.lod_group]
//! measure = "distance"   # or "screen_size", as when left out
//! cull = false           # keep the last level past its end
//! levels = [{ obj = 3, until = 20.0 }, { obj = 4, until = 60.0 }]
//! ```
//!
//! Measured by screen size, `until` is the fraction of the screen's height,
//! from 0 to 1, that a level shows down to; by distance, the metres it
//! shows out to. See [`LodGroup`].

use anyhow::Result;
use serde::{Deserialize, Serialize};
use voxear_ecs::Entity;
use voxear_render::{LodGroup, LodLevel, LodMeasure};

use crate::prefab::ObjPath;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LodGroupData {
    #[serde(default, skip_serializing_if = "is_screen_size")]
    pub measure: LodMeasure,
    /// Whether nothing shows past the last level.
    #[serde(default = "yes", skip_serializing_if = "is_yes")]
    pub cull: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub levels: Vec<LodLevelData>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LodLevelData {
    /// The Obj the level shows. Left out, the level shows nothing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub obj: Option<FileRef>,
    pub until: f32,
}

/// An Obj in the same file: its id, or its path inside a prefab instance,
/// such as `"3/2"`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum FileRef {
    Id(u32),
    Path(String),
}

impl FileRef {
    pub fn new(path: &ObjPath) -> Self {
        match path.ids() {
            [id] => Self::Id(*id),
            _ => Self::Path(path.to_string()),
        }
    }

    pub fn path(&self) -> Result<ObjPath> {
        match self {
            Self::Id(id) => Ok(ObjPath::new(*id)),
            Self::Path(text) => ObjPath::parse(text),
        }
    }
}

impl LodGroupData {
    /// A group as a file saves it, each level's Obj where `path` says it is
    /// in that file. A level whose Obj is not being saved is saved showing
    /// nothing, like a missing reference in Unity.
    pub fn of(group: &LodGroup, path: &dyn Fn(Entity) -> Option<ObjPath>) -> Self {
        let level = |level: &LodLevel| LodLevelData {
            obj: level.obj.and_then(path).map(|path| FileRef::new(&path)),
            until: (level.until * 1e4).round() / 1e4,
        };
        Self { measure: group.measure, cull: group.cull, levels: group.levels.iter().map(level).collect() }
    }

    /// The component, each level's Obj found by `lookup`.
    pub fn bind(&self, lookup: &dyn Fn(&ObjPath) -> Option<Entity>) -> LodGroup {
        let level = |level: &LodLevelData| LodLevel {
            obj: level.obj.as_ref().and_then(|obj| obj.path().ok()).and_then(|path| lookup(&path)),
            until: level.until,
        };
        LodGroup { measure: self.measure, cull: self.cull, levels: self.levels.iter().map(level).collect() }
    }

    /// Every Obj its levels refer to.
    pub fn refs(&self) -> Result<Vec<ObjPath>> {
        self.levels.iter().filter_map(|level| level.obj.as_ref()).map(FileRef::path).collect()
    }

    /// Rewrites each level's reference, as prefabs move paths into the file
    /// that holds an instance.
    pub fn map_refs(&mut self, f: &dyn Fn(&ObjPath) -> ObjPath) -> Result<()> {
        for obj in self.levels.iter_mut().filter_map(|level| level.obj.as_mut()) {
            *obj = FileRef::new(&f(&obj.path()?));
        }
        Ok(())
    }

    /// The whole group as an override writes it. Every value is written,
    /// since an override merges into the prefab's and a value left out
    /// would keep the prefab's.
    pub fn whole(&self) -> Result<toml::Value> {
        let mut table = toml::Table::try_from(self)?;
        table.insert("measure".into(), toml::Value::try_from(self.measure)?);
        table.insert("cull".into(), self.cull.into());
        table.insert("levels".into(), toml::Value::try_from(&self.levels)?);
        Ok(toml::Value::Table(table))
    }
}

fn is_screen_size(measure: &LodMeasure) -> bool {
    *measure == LodMeasure::ScreenSize
}

fn yes() -> bool {
    true
}

fn is_yes(value: &bool) -> bool {
    *value
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_read_and_write_as_references_in_the_file() {
        let text = r#"
            measure = "distance"
            levels = [{ obj = 3, until = 20.0 }, { obj = "4/2", until = 60.0 }, { until = 90.0 }]
        "#;
        let data: LodGroupData = toml::from_str(text).expect("a LOD Group");
        assert_eq!(data.measure, LodMeasure::Distance);
        assert!(data.cull, "culled past the last level unless it says otherwise");
        let refs = data.refs().expect("references");
        assert_eq!(refs, [ObjPath::new(3), ObjPath::parse("4/2").expect("a path")]);

        let near = Entity::from_bits(7);
        let group = data.bind(&|path| (*path == ObjPath::new(3)).then_some(near));
        assert_eq!(group.levels.iter().map(|l| l.obj).collect::<Vec<_>>(), [Some(near), None, None]);
        let saved = LodGroupData::of(&group, &|entity| (entity == near).then(|| ObjPath::new(9)));
        assert_eq!(saved.levels[0].obj, Some(FileRef::Id(9)));
        assert_eq!(toml::to_string(&saved).expect("written").matches("obj").count(), 1);

        let whole = data.whole().expect("an override");
        assert_eq!(whole.get("cull").and_then(toml::Value::as_bool), Some(true));
    }
}
