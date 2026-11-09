//! A World Generator in the Inspector: its preset, seed, scale, sea level
//! and height first, which are all most worlds need; then, closed until
//! opened, its fields, its warp, the curves that shape its ground and its
//! crags and bumps. Picking a preset reshapes it, keeping its seed and
//! scale; changing anything of its shape makes it a preset of its own.
//! Each change is saved as it is made, and terrains generated from it are
//! made again (see `editor::world_gens`).

use voxear_core::curve::Curve;
use voxear_terrain::generate::{Field, Preset, WorldGen, smooth};
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
    fn curve(self, world: &mut WorldGen) -> &mut Curve {
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
    fn field(self, world: &mut WorldGen) -> &mut Field {
        match self {
            Which::Continents => &mut world.continents,
            Which::Ruggedness => &mut world.ruggedness,
            Which::Ridges => &mut world.ridges,
        }
    }
}

/// A generator's fields, and the generator as they show it.
pub struct WorldGenFields {
    pub world: WorldGen,
    preset: NodeId,
    seed: NodeId,
    scale: NodeId,
    sea_level: NodeId,
    height: NodeId,
    /// Each field's size and octaves.
    fields: Vec<(Which, NodeId, NodeId)>,
    sharpness: NodeId,
    warp: [NodeId; 2],
    curves: Vec<(Shaping, CurveField)>,
    crags: NodeId,
    bumps: NodeId,
}

impl WorldGenFields {
    pub fn new(ui: &mut Ui, root: NodeId, world: WorldGen) -> Self {
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
        let bumps = number_field(ui, advanced, "Bumps", world.bumps);
        Self { world, preset, seed, scale, sea_level, height, fields, sharpness, warp, curves, crags, bumps }
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
            (self.sharpness, &mut world.sharpness, 0.0),
            (self.warp[0], &mut world.warp.size, 1.0),
            (self.warp[1], &mut world.warp.strength, 0.0),
            (self.crags, &mut world.crags, 0.0),
            (self.bumps, &mut world.bumps, 0.0),
        ] {
            if let Some(value) = changed_number(ui, field) {
                (*slot, typed) = (value.max(low), true);
            }
        }
        world.sharpness = world.sharpness.min(1.0);
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
