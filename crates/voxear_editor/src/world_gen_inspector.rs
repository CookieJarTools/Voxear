//! A World Generator in the Inspector. One that is a landmass and nothing
//! more shows its preset, seed, scale, sea level and height first, which
//! are all most worlds need; then, closed until opened, its fields, its
//! warp, the curves that shape its ground and its crags and bumps. Picking
//! a preset reshapes it, keeping its seed and scale; changing anything of
//! its shape makes it a preset of its own. A generator with a graph of its
//! own shows its seed, scale and sea level, and a row for each value its
//! graph exposes ([`GraphFields`]). Each change is saved as it is made,
//! and terrains generated from it are made again (see
//! `editor::world_gens`).

use voxear_core::curve::Curve;
use voxear_terrain::generate::{Field, Landmass, Preset, Value, ValueKind, WorldGen, smooth};
use voxear_ui::{NodeId, Ui};

use crate::{
    asset_inspector::part,
    curve_fields::CurveField,
    inspector::{number, prop},
};

/// What was done to a generator's fields this frame.
pub enum WorldGenChange {
    /// A value typed, or a curve's key moved: saved as it is shown.
    Typed,
    /// A preset picked: saved, and shown again.
    Shape,
}

/// One of a generator's curves, as its row names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Shaping {
    Coast,
    Inland,
    Relief,
    Peaks,
}

const SHAPING: [(Shaping, &str); 4] =
    [(Shaping::Coast, "Coast"), (Shaping::Inland, "Inland"), (Shaping::Relief, "Relief"), (Shaping::Peaks, "Peaks")];

impl Shaping {
    fn curve(self, world: &mut Landmass) -> &mut Curve {
        match self {
            Shaping::Coast => &mut world.coast,
            Shaping::Inland => &mut world.inland,
            Shaping::Relief => &mut world.relief,
            Shaping::Peaks => &mut world.peaks,
        }
    }

    /// What it is with no keys: nothing for heights, all for shares.
    fn empty(self) -> f32 {
        match self {
            Shaping::Coast | Shaping::Peaks => 0.0,
            Shaping::Inland | Shaping::Relief => 1.0,
        }
    }
}

/// One of a generator's fields, as its rows name it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Which {
    Continents,
    Ruggedness,
    Ridges,
}

impl Which {
    fn field(self, world: &mut Landmass) -> &mut Field {
        match self {
            Which::Continents => &mut world.continents,
            Which::Ruggedness => &mut world.ruggedness,
            Which::Ridges => &mut world.ridges,
        }
    }
}

/// The fields of a generator that is one landmass, and the landmass as
/// they show it.
pub struct WorldGenFields {
    pub world: Landmass,
    preset: NodeId,
    seed: NodeId,
    scale: NodeId,
    sea_level: NodeId,
    height: NodeId,
    /// Its temperature and humidity on average.
    warmth: NodeId,
    wet: NodeId,
    /// Its climate's size, how far its temperature and humidity stray, and
    /// how fast it cools up high.
    climate: [NodeId; 4],
    /// Each field's size and octaves.
    fields: Vec<(Which, NodeId, NodeId)>,
    sharpness: NodeId,
    warp: [NodeId; 2],
    curves: Vec<(Shaping, CurveField)>,
    crags: NodeId,
    /// How deeply water cuts gullies, how far apart, and in how many octaves.
    erosion: [NodeId; 3],
    bumps: NodeId,
}

impl WorldGenFields {
    pub fn new(ui: &mut Ui, root: NodeId, world: Landmass) -> Self {
        let rows = part(ui, root, "World Generator", "sprout");
        let mut presets: Vec<&str> = Preset::ALL.iter().map(|preset| preset.name()).collect();
        presets.push("Custom");
        let picked = world.preset().and_then(|p| Preset::ALL.iter().position(|&q| q == p)).unwrap_or(presets.len() - 1);
        let preset = choice(ui, rows, "Preset", &presets, picked);
        let seed = number_field(ui, rows, "Seed", 0.0);
        ui.set_text(seed, &world.seed.to_string());
        let scale = number_field(ui, rows, "Scale", world.scale);
        let sea_level = number_field(ui, rows, "Sea level", world.sea_level);
        let height = number_field(ui, rows, "Height", world.height);
        let warmth = number_field(ui, rows, "Temperature", world.climate.temperature);
        let wet = number_field(ui, rows, "Humidity", world.climate.humidity);

        let line = ui.panel(root, voxear_ui::Style::default());
        ui.add_class(line, "divider");
        let foldout = ui.foldout(root, "Advanced", false);
        ui.add_class(foldout, "section");
        ui.set_foldout_icon(foldout, "settings");
        let advanced = ui.foldout_content(foldout).expect("a foldout has content");
        let mut fields = Vec::new();
        for (which, title, field) in [
            (Which::Continents, "Continents", world.continents),
            (Which::Ruggedness, "Ruggedness", world.ruggedness),
            (Which::Ridges, "Ridges", world.ridges),
        ] {
            let size = number_field(ui, advanced, &format!("{title} size"), field.size);
            let octaves = number_field(ui, advanced, &format!("{title} octaves"), field.octaves as f32);
            fields.push((which, size, octaves));
        }
        let sharpness = number_field(ui, advanced, "Sharpness", world.sharpness);
        let warp = [
            number_field(ui, advanced, "Warp size", world.warp.size),
            number_field(ui, advanced, "Warp strength", world.warp.strength),
        ];
        let mut curves = Vec::new();
        for (shaping, title) in SHAPING {
            let row = prop(ui, advanced, title);
            let field = CurveField::new(ui, row);
            curves.push((shaping, field));
        }
        let crags = number_field(ui, advanced, "Crags", world.crags);
        let erosion = [
            number_field(ui, advanced, "Erosion", world.erosion.strength),
            number_field(ui, advanced, "Gully spacing", world.erosion.spacing),
            number_field(ui, advanced, "Gully octaves", world.erosion.octaves as f32),
        ];
        let bumps = number_field(ui, advanced, "Bumps", world.bumps);
        let climate = [
            number_field(ui, advanced, "Climate size", world.climate.size),
            number_field(ui, advanced, "Temperature range", world.climate.temperature_range),
            number_field(ui, advanced, "Humidity range", world.climate.humidity_range),
            number_field(ui, advanced, "Cooling", world.climate.cooling),
        ];
        Self {
            world,
            preset,
            seed,
            scale,
            sea_level,
            height,
            warmth,
            wet,
            climate,
            fields,
            sharpness,
            warp,
            curves,
            crags,
            erosion,
            bumps,
        }
    }

    /// What was done this frame, the generator changed to match; and its
    /// curves drawn as they are.
    pub fn update(&mut self, ui: &mut Ui) -> Option<WorldGenChange> {
        if ui.changed(self.preset) {
            if let Some(&preset) = Preset::ALL.get(ui.selected_option(self.preset)) {
                self.world = self.world.with_preset(preset);
                return Some(WorldGenChange::Shape);
            }
        }
        let mut typed = false;
        let world = &mut self.world;
        if ui.changed(self.seed)
            && let Ok(seed) = ui.text(self.seed).trim().parse::<u32>()
        {
            (world.seed, typed) = (seed, true);
        }
        for (field, slot, low) in [
            (self.scale, &mut world.scale, 0.01),
            (self.sea_level, &mut world.sea_level, f32::MIN),
            (self.height, &mut world.height, 0.0),
            (self.warmth, &mut world.climate.temperature, -100.0),
            (self.wet, &mut world.climate.humidity, 0.0),
            (self.climate[0], &mut world.climate.size, 1.0),
            (self.climate[1], &mut world.climate.temperature_range, 0.0),
            (self.climate[2], &mut world.climate.humidity_range, 0.0),
            (self.climate[3], &mut world.climate.cooling, 0.0),
            (self.sharpness, &mut world.sharpness, 0.0),
            (self.warp[0], &mut world.warp.size, 1.0),
            (self.warp[1], &mut world.warp.strength, 0.0),
            (self.crags, &mut world.crags, 0.0),
            (self.erosion[0], &mut world.erosion.strength, 0.0),
            (self.erosion[1], &mut world.erosion.spacing, 1.0),
            (self.bumps, &mut world.bumps, 0.0),
        ] {
            if let Some(value) = changed_number(ui, field) {
                (*slot, typed) = (value.max(low), true);
            }
        }
        world.sharpness = world.sharpness.min(1.0);
        world.climate.humidity = world.climate.humidity.min(1.0);
        if let Some(value) = changed_number(ui, self.erosion[2]) {
            (world.erosion.octaves, typed) = (value.clamp(0.0, 8.0) as u32, true);
        }
        for &(which, size, octaves) in &self.fields {
            if let Some(value) = changed_number(ui, size) {
                (which.field(world).size, typed) = (value.max(1.0), true);
            }
            if let Some(value) = changed_number(ui, octaves) {
                (which.field(world).octaves, typed) = (value.clamp(1.0, 12.0) as u32, true);
            }
        }
        for (shaping, field) in &self.curves {
            let curve = shaping.curve(world);
            if let Some(dragged) = field.drag(ui, curve) {
                (*curve, typed) = (dragged, true);
            }
            let (curve, empty) = (*curve, shaping.empty());
            field.draw_by(ui, &curve, &|t| smooth(&curve, t, empty));
        }
        // Its shape its own once changed: no longer a preset's.
        let picked = world.preset().and_then(|p| Preset::ALL.iter().position(|&q| q == p)).unwrap_or(Preset::ALL.len());
        if ui.selected_option(self.preset) != picked {
            ui.set_selected_option(self.preset, picked);
        }
        typed.then_some(WorldGenChange::Typed)
    }
}

/// One row of a graph's exposed values: which row it is, and its field.
enum Shown {
    Number(usize, NodeId),
    Choice(usize, NodeId),
    Curve(usize, CurveField, f32),
}

/// The fields of a generator with a graph of its own, and the generator as
/// they show it: its seed, scale and sea level, then the values its graph
/// exposes.
pub struct GraphFields {
    pub world: WorldGen,
    seed: NodeId,
    scale: NodeId,
    sea_level: NodeId,
    shown: Vec<Shown>,
}

impl GraphFields {
    pub fn new(ui: &mut Ui, root: NodeId, world: WorldGen) -> Self {
        let rows = part(ui, root, "World Generator", "sprout");
        let seed = number_field(ui, rows, "Seed", 0.0);
        ui.set_text(seed, &world.seed().to_string());
        let scale = number_field(ui, rows, "Scale", world.scale());
        let sea_level = number_field(ui, rows, "Sea level", world.sea_level());
        let mut shown = Vec::new();
        let graph = world.graph();
        for (row, exposed) in graph.exposed.iter().enumerate() {
            let spec = graph.node(exposed.node).and_then(|node| node.kind.values().get(exposed.value));
            let (Some(spec), Some(value)) = (spec, graph.exposed_value(row)) else { continue };
            shown.push(match (spec.kind, value) {
                (ValueKind::Choice(choices), Value::Choice(picked)) => {
                    Shown::Choice(row, choice(ui, rows, &exposed.label, choices, picked))
                }
                (ValueKind::Curve { empty }, _) => {
                    let field = prop(ui, rows, &exposed.label);
                    Shown::Curve(row, CurveField::new(ui, field), empty)
                }
                _ => Shown::Number(row, number_field(ui, rows, &exposed.label, value.number().unwrap_or(0.0))),
            });
        }
        Self { world, seed, scale, sea_level, shown }
    }

    /// Whether anything was changed this frame, the generator changed to
    /// match; and its curves drawn as they are.
    pub fn update(&mut self, ui: &mut Ui) -> bool {
        let before = self.world.clone();
        if ui.changed(self.seed)
            && let Ok(seed) = ui.text(self.seed).trim().parse::<u32>()
        {
            self.world = self.world.with_seed(seed);
        }
        if let Some(scale) = changed_number(ui, self.scale) {
            self.world = self.world.with_scale(scale.max(0.01));
        }
        if let Some(sea_level) = changed_number(ui, self.sea_level) {
            self.world = self.world.with_sea_level(sea_level);
        }
        for shown in &self.shown {
            let set = match shown {
                Shown::Number(row, field) => changed_number(ui, *field).map(|number| (*row, Value::Number(number))),
                Shown::Choice(row, field) => {
                    ui.changed(*field).then(|| (*row, Value::Choice(ui.selected_option(*field))))
                }
                Shown::Curve(row, field, empty) => {
                    let Some(Value::Curve(curve)) = self.world.graph().exposed_value(*row) else { continue };
                    let dragged = field.drag(ui, &curve);
                    let now = dragged.unwrap_or(curve);
                    field.draw_by(ui, &now, &|t| smooth(&now, t, *empty));
                    dragged.map(|curve| (*row, Value::Curve(curve)))
                }
            };
            if let Some(world) = set.and_then(|(row, value)| self.world.with_exposed(row, value)) {
                self.world = world;
            }
        }
        self.world != before
    }
}

fn choice(ui: &mut Ui, rows: NodeId, label: &str, options: &[&str], selected: usize) -> NodeId {
    let row = prop(ui, rows, label);
    let dropdown = ui.dropdown(row, options, selected);
    ui.add_class(dropdown, "value-choice");
    dropdown
}

fn number_field(ui: &mut Ui, rows: NodeId, label: &str, value: f32) -> NodeId {
    let row = prop(ui, rows, label);
    let field = ui.text_field(row, &number(value));
    ui.add_class(field, "value-input");
    ui.add_class(field, "number-input");
    field
}

fn changed_number(ui: &Ui, field: NodeId) -> Option<f32> {
    ui.changed(field).then(|| ui.text(field).trim().parse().ok()).flatten()
}
