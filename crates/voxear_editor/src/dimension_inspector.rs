//! A dimension (`.dimension`) in the Inspector: its settings, its file's
//! `[dimension]` table. **Kind** is the easy way: Overworld, Dungeon or
//! Caves, each setting good values for the rest, which show what it gives.
//! The full way is each of the rest: whether it has a **Sky** and
//! **Weather**, whose **Clock** it keeps, how long it is **Kept Loaded**
//! after nothing needs it, and its **Instances**. A value changed is written
//! into the file as it is made; a change of Kind puts the rest back to what
//! the new kind gives.

use voxear_engine::dimensions::{DimensionClock, DimensionInstances, DimensionKind, DimensionSettings, KEEP_LOADED};
use voxear_ui::{NodeId, Ui};

use crate::{
    asset_inspector::part,
    biome_inspector::{changed_number, number_row},
    inspector::prop,
};

const KINDS: [(DimensionKind, &str); 3] =
    [(DimensionKind::Overworld, "Overworld"), (DimensionKind::Dungeon, "Dungeon"), (DimensionKind::Caves, "Caves")];
const CLOCKS: [(DimensionClock, &str); 3] =
    [(DimensionClock::Level, "Level"), (DimensionClock::Own, "Own"), (DimensionClock::None, "None")];
const INSTANCES: [(DimensionInstances, &str); 2] =
    [(DimensionInstances::One, "One"), (DimensionInstances::EachSite, "Each Site")];

/// What each setting does, as its label's tooltip says, before its default.
const KIND_TIP: &str = "What the dimension is for, which sets good values for the rest. Overworld: open land under \
                        a sky of its own, with weather, on the level's clock. Dungeon: enclosed, one for each place \
                        that opens it. Caves: enclosed, only one.";
const SKY_TIP: &str = "Whether it has a sky. Without one it is enclosed: dark all round, no sun and no light from the \
                       sky, only its own lamps and glowing blocks.";
const WEATHER_TIP: &str = "Whether its lighting's weather runs in it, with a sky. Without, it is always clear.";
const CLOCK_TIP: &str = "Whose time of day it keeps. Level: its sun stands where the level's does. Own: its first \
                         Climate's. None: its sun stays where it was put.";
const KEEP_TIP: &str = "How many seconds it stays loaded after nothing needs it any more: no camera in it, and no \
                        open Portal into it.";
const INSTANCES_TIP: &str = "One, wherever it is opened from; or Each Site, one for each place that opens it, each \
                             made from its own seed, as each dungeon is its own.";

/// A setting's tooltip: what it does, then what it is with nothing said,
/// as the Project Settings' tooltips end.
fn tip(what: &str, default: &str) -> String {
    format!("{what}\n\nDefault: {default}")
}

fn on_off(on: bool) -> &'static str {
    if on { "On" } else { "Off" }
}

/// A dimension's fields, and its settings as they show them.
pub struct DimensionFields {
    pub settings: DimensionSettings,
    kind: NodeId,
    sky: NodeId,
    weather: NodeId,
    clock: NodeId,
    keep_loaded: NodeId,
    instances: NodeId,
}

impl DimensionFields {
    /// The fields of `settings` under `root`.
    pub fn new(ui: &mut Ui, root: NodeId, settings: DimensionSettings) -> Self {
        let kind_of = DimensionSettings::of(settings.kind);
        let rows = part(ui, root, "Dimension", "globe");
        let dropdown = |ui: &mut Ui, label: &str, tooltip: String, options: &[&str], at: usize| {
            let row = prop(ui, rows, label);
            ui.set_tooltip(row, &tooltip, None);
            let field = ui.dropdown(row, options, at);
            ui.add_class(field, "value-choice");
            field
        };
        let toggle = |ui: &mut Ui, label: &str, tooltip: String, on: bool| {
            let row = prop(ui, rows, label);
            ui.set_tooltip(row, &tooltip, None);
            let field = ui.toggle(row, "", on);
            ui.add_class(field, "checkbox");
            field
        };
        let at = |found: Option<usize>| found.unwrap_or_default();
        let kind = dropdown(
            ui,
            "Kind",
            tip(KIND_TIP, "Overworld"),
            &KINDS.map(|(_, name)| name),
            at(KINDS.iter().position(|(kind, _)| *kind == settings.kind)),
        );
        let sky =
            toggle(ui, "Sky", tip(SKY_TIP, &format!("{}, as its kind gives", on_off(kind_of.sky()))), settings.sky());
        let weather = toggle(
            ui,
            "Weather",
            tip(WEATHER_TIP, &format!("{}, as its kind gives", on_off(kind_of.weather()))),
            settings.weather(),
        );
        let clock_default =
            CLOCKS.iter().find(|(clock, _)| *clock == kind_of.clock()).map_or("Level", |(_, name)| name);
        let clock = dropdown(
            ui,
            "Clock",
            tip(CLOCK_TIP, &format!("{clock_default}, as its kind gives")),
            &CLOCKS.map(|(_, name)| name),
            at(CLOCKS.iter().position(|(clock, _)| *clock == settings.clock())),
        );
        let keep_loaded = number_row(ui, rows, "Keep Loaded", "s", settings.keep_loaded());
        if let Some(row) = ui.parent(keep_loaded) {
            ui.set_tooltip(row, &tip(KEEP_TIP, &format!("{KEEP_LOADED} s")), None);
        }
        let instances_default =
            INSTANCES.iter().find(|(one, _)| *one == kind_of.instances()).map_or("One", |(_, name)| name);
        let instances = dropdown(
            ui,
            "Instances",
            tip(INSTANCES_TIP, &format!("{instances_default}, as its kind gives")),
            &INSTANCES.map(|(_, name)| name),
            at(INSTANCES.iter().position(|(one, _)| *one == settings.instances())),
        );
        Self { settings, kind, sky, weather, clock, keep_loaded, instances }
    }

    /// Whether a setting changed this frame, the settings changed to match:
    /// a value set says it outright, and a new Kind clears the rest.
    pub fn update(&mut self, ui: &Ui) -> bool {
        let settings = &mut self.settings;
        if ui.changed(self.kind)
            && let Some(&(kind, _)) = KINDS.get(ui.selected_option(self.kind))
            && kind != settings.kind
        {
            *settings = DimensionSettings::of(kind);
            return true;
        }
        if ui.changed(self.sky) {
            settings.sky = Some(ui.is_checked(self.sky));
            return true;
        }
        if ui.changed(self.weather) {
            settings.weather = Some(ui.is_checked(self.weather));
            return true;
        }
        if ui.changed(self.clock)
            && let Some(&(clock, _)) = CLOCKS.get(ui.selected_option(self.clock))
        {
            settings.clock = Some(clock);
            return true;
        }
        if ui.changed(self.instances)
            && let Some(&(instances, _)) = INSTANCES.get(ui.selected_option(self.instances))
        {
            settings.instances = Some(instances);
            return true;
        }
        if let Some(seconds) = changed_number(ui, self.keep_loaded) {
            settings.keep_loaded = Some(seconds.clamp(0.0, 3600.0));
            return true;
        }
        false
    }
}

/// A new dimension's file of `kind`: its settings, and a Portal, Way Back,
/// for a Portal in the level to lead into.
pub fn new_file(kind: DimensionKind) -> String {
    let settings = DimensionSettings::of(kind);
    let table = toml::Table::try_from(&settings).unwrap_or_default();
    let settings = toml::to_string(&table).unwrap_or_default();
    format!(
        "format = 1\n\n[dimension]\n{settings}\n[[obj]]\nid = 1\nname = \"Way Back\"\nposition = [0.0, 0.0, 0.0]\n\n[obj.portal]\nsize = [3.0, 4.0]\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_dimension_of_a_kind_reads_back_as_that_kind_with_a_portal() {
        for (kind, _) in KINDS {
            let text = new_file(kind);
            let file = voxear_engine::dimensions::DimensionFile::parse(&text, "New.dimension").unwrap();
            assert_eq!(file.settings, DimensionSettings::of(kind), "{text}");
            assert!(file.scene.objs[0].portal.is_some());
        }
    }

    #[test]
    fn a_tooltip_ends_with_its_default() {
        assert_eq!(tip("Rooms ring.", "Off"), "Rooms ring.\n\nDefault: Off");
    }
}
