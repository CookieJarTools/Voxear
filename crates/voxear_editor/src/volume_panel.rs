//! A Post Volume's own settings, at the foot of its Inspector section: the
//! post-processing settings, a tab for each group as the Lighting page has
//! them, each row empty where the volume leaves the setting to the level.
//! A change is written into the volume's overrides, as a scene's `[post]`
//! writes them, as one step to undo.

use voxear_ecs::Entity;
use voxear_engine::Game;
use voxear_render::PostVolume;
use voxear_ui::{NodeId, Style, Ui};

use crate::lighting_panel::LightingFields;

pub struct VolumePanel {
    fields: LightingFields,
}

impl VolumePanel {
    /// The settings of the Post Volume on `entity`, under `rows`, on the tab
    /// of `group` where it has one.
    pub fn new(ui: &mut Ui, rows: NodeId, game: &Game, entity: Entity, group: Option<&'static str>) -> Self {
        let heading = ui.add_label(rows, "Settings");
        ui.add_class(heading, "settings-group");
        ui.set_tooltip(
            heading,
            "What the volume changes where the camera is in it. An empty row is the level's own",
            None,
        );
        let tabs = ui.panel(rows, Style::row());
        ui.add_class(tabs, "lighting-asset-tabs");
        let body = ui.panel(rows, Style::column());
        let mut fields = LightingFields::for_volume(ui, tabs, body);
        fields.set_pictures(ui, crate::lighting_panel::pictures(game));
        if let Some(group) = group {
            fields.set_group(ui, group);
        }
        let mut panel = Self { fields };
        panel.show(ui, game, entity);
        panel
    }

    fn show(&mut self, ui: &mut Ui, game: &Game, entity: Entity) {
        let overrides = game.engine.world.get::<PostVolume>(entity).map(|volume| volume.overrides.clone());
        let mut values = toml::Table::new();
        values.insert("post".into(), toml::Value::Table(overrides.clone().unwrap_or_default()));
        self.fields.show_values(ui, overrides.is_some(), values);
    }

    /// A setting's row, by its title.
    pub fn row_of(&self, title: &str) -> Option<NodeId> {
        self.fields.row_of(title)
    }

    /// The tab shown, to show again when the section is built again.
    pub fn group(&self) -> &'static str {
        self.fields.group()
    }

    /// Writes a setting changed this frame into the volume; whether one was.
    /// A value the volume cannot take leaves it as it was.
    pub fn update(&mut self, ui: &mut Ui, game: &mut Game, entity: Entity) -> bool {
        let Some((path, value)) = self.fields.changed(ui) else { return false };
        let Some(volume) = game.engine.world.get_mut::<PostVolume>(entity) else { return false };
        // Rows are laid out as a lighting file's, under `post`.
        let Some(path) = path.strip_prefix(&["post"][..]) else { return false };
        let before = volume.overrides.clone();
        set(&mut volume.overrides, path, value.and_then(|value| read(&value)));
        if let Err(error) = volume.check() {
            tracing::error!("{error:#}");
            volume.overrides = before;
            return false;
        }
        self.show(ui, game, entity);
        true
    }
}

/// A value as `toml_edit` writes it, as `toml` reads it.
fn read(value: &toml_edit::Value) -> Option<toml::Value> {
    let table: toml::Table = toml::from_str(&format!("value = {value}")).ok()?;
    table.get("value").cloned()
}

/// Sets the setting at `path` in `table`, or takes it out with `None`, with
/// each table left empty.
fn set(table: &mut toml::Table, path: &[&str], value: Option<toml::Value>) {
    let Some((first, rest)) = path.split_first() else { return };
    if rest.is_empty() {
        match value {
            Some(value) => drop(table.insert((*first).to_string(), value)),
            None => drop(table.remove(*first)),
        }
        return;
    }
    let inner = table.entry(*first).or_insert_with(|| toml::Value::Table(toml::Table::new()));
    let Some(inner) = inner.as_table_mut() else { return };
    set(inner, rest, value);
    if inner.is_empty() {
        table.remove(*first);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_setting_goes_into_its_section_and_its_section_goes_with_its_last_setting() {
        let mut overrides = toml::Table::new();
        set(&mut overrides, &["bloom", "intensity"], read(&0.3.into()));
        set(
            &mut overrides,
            &["grading", "lift"],
            read(&toml_edit::Value::Array([0.1, 0.0, 0.0].into_iter().collect())),
        );
        let volume = PostVolume { overrides: overrides.clone(), ..PostVolume::default() };
        volume.check().unwrap();
        assert_eq!(overrides["bloom"]["intensity"].as_float(), Some(0.3));
        set(&mut overrides, &["bloom", "intensity"], None);
        assert!(!overrides.contains_key("bloom"), "{overrides:?}");
        assert!(overrides.contains_key("grading"));
    }
}
