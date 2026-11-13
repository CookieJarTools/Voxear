//! A Block in the Inspector, laid out as a material's is. Its **Look**: its
//! colour, the material every face is drawn with and those its top and
//! bottom have of their own, its shape (a cube, a cube with tufts of leaves,
//! or a vine), and which liquid it is (a liquid is drawn in its colour
//! alone, so its materials and its shape go). **Breaking**: how long it takes
//! by hand, the kind of tool that breaks it fast, and the tier of that tool
//! that gets its drops. Its **Drops**, each an item, how many and how
//! often. Its **Sounds**, and the **Data** asset a game gives it. Each
//! change is saved as it is made, and every world of blocks listing the
//! block takes it up at once (see `editor::blocks`).

use voxear_terrain::blocks::block::{BlockDef, BlockDrop, BlockHeat, BlockShape, Liquid, MAX_HEAT_REACH};
use voxear_ui::{DragSource, NodeId, PickedColor, Style, Ui};

use crate::{
    asset_inspector::{Slot, part, show_slot, slot},
    collection::{self, Collection},
    inspector::{color_row, number, prop},
};

/// What was done to a block's fields this frame.
pub enum BlockChange {
    /// A value typed: saved as it is shown.
    Typed,
    /// What it shows changed: saved, and shown again.
    Shape,
    /// The arrow of a field naming an asset, to open the picker under.
    Browse(BlockRef, NodeId),
    /// Something dragged onto such a field.
    Drop(BlockRef, DragSource),
    /// A click on a field naming an asset: show it in the Project panel.
    Ping(String),
    /// Make Item: an item that places the block, made and dropped by it.
    MakeItem,
}

/// One of a block's fields naming an asset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockRef {
    Material,
    Top,
    Bottom,
    /// The kind of tool that breaks it fast.
    Tool,
    /// The item of the drop at this place in its list.
    Drop(usize),
    BreakSound,
    PlaceSound,
    StepSound,
    Data,
}

impl BlockRef {
    const FACES: [(BlockRef, &'static str); 3] =
        [(BlockRef::Material, "Material"), (BlockRef::Top, "Top"), (BlockRef::Bottom, "Bottom")];
    const SOUNDS: [(BlockRef, &'static str); 3] =
        [(BlockRef::BreakSound, "Break"), (BlockRef::PlaceSound, "Place"), (BlockRef::StepSound, "Step")];

    /// The reference the field holds in `block`.
    fn of(self, block: &BlockDef) -> Option<&str> {
        match self {
            BlockRef::Material => block.material.as_deref(),
            BlockRef::Top => block.top.as_deref(),
            BlockRef::Bottom => block.bottom.as_deref(),
            BlockRef::Tool => block.tool.as_deref(),
            BlockRef::Drop(at) => block.drops.get(at)?.item.as_deref(),
            BlockRef::BreakSound => block.sounds.broken.as_deref(),
            BlockRef::PlaceSound => block.sounds.placed.as_deref(),
            BlockRef::StepSound => block.sounds.step.as_deref(),
            BlockRef::Data => block.data.as_deref(),
        }
    }

    /// Puts `reference` in the field of `block`.
    pub fn set(self, block: &mut BlockDef, reference: Option<String>) {
        match self {
            BlockRef::Material => block.material = reference,
            BlockRef::Top => block.top = reference,
            BlockRef::Bottom => block.bottom = reference,
            BlockRef::Tool => block.tool = reference,
            BlockRef::Drop(at) => {
                if let Some(drop) = block.drops.get_mut(at) {
                    drop.item = reference;
                }
            }
            BlockRef::BreakSound => block.sounds.broken = reference,
            BlockRef::PlaceSound => block.sounds.placed = reference,
            BlockRef::StepSound => block.sounds.step = reference,
            BlockRef::Data => block.data = reference,
        }
    }

    /// What the field says it holds when it holds nothing, and its icon.
    fn kind(self) -> (&'static str, &'static str) {
        match self {
            BlockRef::Material | BlockRef::Top | BlockRef::Bottom => ("Material", "material"),
            BlockRef::Tool => ("Tool Kind", "file"),
            BlockRef::Drop(_) => ("Item", "file"),
            BlockRef::BreakSound | BlockRef::PlaceSound | BlockRef::StepSound => ("Sound", "audio"),
            BlockRef::Data => ("Data Asset", "file"),
        }
    }
}

const LIQUIDS: [(Liquid, &str); 2] = [(Liquid::None, "None"), (Liquid::Water, "Water")];

/// One drop's row: the fields of how many, from and to, and how often.
struct DropRow {
    count: [NodeId; 2],
    chance: NodeId,
    remove: Option<NodeId>,
}

/// A block's fields, and the block as they show it.
pub struct BlockFields {
    pub block: BlockDef,
    color: [NodeId; 3],
    swatch: NodeId,
    liquid: NodeId,
    hardness: NodeId,
    unbreakable: NodeId,
    /// Whether it sways in the wind; none for a liquid.
    sways: Option<NodeId>,
    /// Its shape; none for a liquid.
    shape: Option<NodeId>,
    tier: NodeId,
    /// Whether it is hot or cold, and how much and how far when it is.
    heat: NodeId,
    celsius: Option<NodeId>,
    reach: Option<NodeId>,
    drops: Collection,
    drop_rows: Vec<DropRow>,
    make_item: Option<NodeId>,
    /// Every field naming an asset, with the path of what it names.
    slots: Vec<(BlockRef, Slot, Option<String>)>,
}

impl BlockFields {
    /// The fields of `block` under `root`; `path_of` gives the project path
    /// of the asset a reference names, and `carried` says whether an item
    /// places the block already.
    pub fn new(
        ui: &mut Ui,
        root: NodeId,
        block: BlockDef,
        path_of: &dyn Fn(&str) -> Option<String>,
        carried: bool,
    ) -> Self {
        let mut slots = Vec::new();
        let mut field = |ui: &mut Ui, rows: NodeId, which: BlockRef, label: &str| {
            let (kind, icon) = which.kind();
            let shown = slot(ui, rows, label, icon);
            let path = which.of(&block).and_then(path_of);
            show_slot(ui, shown, path.as_deref(), kind);
            slots.push((which, shown, path));
        };

        let rows = part(ui, root, "Look", "block");
        let (color, swatch) = color_row(ui, rows, "Color");
        for (shown, value) in color.iter().zip(block.color) {
            ui.set_text(*shown, &number(value));
        }
        if block.liquid.is_none() {
            for (which, label) in BlockRef::FACES {
                field(ui, rows, which, label);
            }
        }
        let sways = block.liquid.is_none().then(|| {
            let row = prop(ui, rows, "Sways in wind");
            let tip = "Its corners that touch no still block move with the level's wind, as leaves do. A block drawn \
                       with a foliage material sways without this";
            ui.set_tooltip(row, tip, None);
            ui.toggle(row, "", block.sways)
        });
        let shape = block.liquid.is_none().then(|| {
            let row = prop(ui, rows, "Shape");
            let tip = "Cube: a block as most are. Bushy: a cube with tufts of leaves reaching out past it. Vine: thin \
                       sheets on the blocks beside it, or hanging; walked through";
            ui.set_tooltip(row, tip, None);
            let picked = BlockShape::ALL.iter().position(|shape| *shape == block.shape).unwrap_or_default();
            let shape = ui.dropdown(row, &BlockShape::ALL.map(BlockShape::name), picked);
            ui.add_class(shape, "value-choice");
            shape
        });
        let row = prop(ui, rows, "Liquid");
        let picked = LIQUIDS.iter().position(|(liquid, _)| *liquid == block.liquid).unwrap_or_default();
        let liquid = ui.dropdown(row, &LIQUIDS.map(|(_, name)| name), picked);
        ui.add_class(liquid, "value-choice");

        let rows = part(ui, root, "Breaking", "settings");
        let hardness = number_field(ui, rows, "Hardness", block.hardness);
        let row = prop(ui, rows, "Unbreakable");
        let unbreakable = ui.toggle(row, "", block.unbreakable);
        field(ui, rows, BlockRef::Tool, "Tool");
        let tier = number_field(ui, rows, "Tier", block.tier as f32);

        let rows = part(ui, root, "Heat", "sun");
        let row = prop(ui, rows, "Hot or Cold");
        let heat = ui.toggle(row, "", block.heat.is_some());
        let celsius = block.heat.map(|heat| number_field(ui, rows, "Temperature", heat.celsius));
        let reach = block.heat.map(|heat| number_field(ui, rows, "Reach", heat.reach));

        let rows = part(ui, root, "Drops", "box");
        let drops = collection::collection(ui, rows, block.drops.len(), true, Some("Add Drop"));
        ui.set_reorderable(drops.items, false);
        ui.set_text(drops.empty, "Drops nothing");
        let mut drop_rows = Vec::new();
        for (place, drop) in block.drops.iter().enumerate() {
            let item = collection::fixed_item(ui, drops.items, place);
            field(ui, item.body, BlockRef::Drop(place), "Item");
            let row = prop(ui, item.body, "Count");
            let pair = ui.panel(row, Style::row());
            ui.add_class(pair, "vector");
            let count = drop.count.map(|count| {
                let shown = ui.text_field(pair, &count.to_string());
                ui.add_class(shown, "value-input");
                ui.add_class(shown, "number-input");
                shown
            });
            let chance = number_field(ui, item.body, "Chance", drop.chance);
            drop_rows.push(DropRow { count, chance, remove: item.remove });
        }
        // A block carried is an item that places it: made here where it has none.
        let make_item =
            drops.footer.filter(|_| !carried).map(|footer| collection::footer_button(ui, footer, "Make Item"));

        let rows = part(ui, root, "Sounds", "audio");
        for (which, label) in BlockRef::SOUNDS {
            field(ui, rows, which, label);
        }
        let rows = part(ui, root, "Data", "file");
        field(ui, rows, BlockRef::Data, "Data");

        let fields =
            Self {
                block,
                color,
                swatch,
                liquid,
                hardness,
                unbreakable,
                sways,
                shape,
                tier,
                heat,
                celsius,
                reach,
                drops,
                drop_rows,
                make_item,
                slots,
            };
        fields.show_swatch(ui);
        fields
    }

    /// What was done this frame, the block changed to match.
    pub fn update(&mut self, ui: &mut Ui) -> Option<BlockChange> {
        for (which, field, path) in &self.slots {
            if let Some(source) = ui.dropped_onto(field.field) {
                return Some(BlockChange::Drop(*which, source));
            }
            if ui.clicked(field.arrow) {
                return Some(BlockChange::Browse(*which, field.field));
            }
            if let Some(path) = path.clone().filter(|_| ui.clicked(field.field)) {
                return Some(BlockChange::Ping(path));
            }
        }
        if self.make_item.is_some_and(|button| ui.clicked(button)) {
            return Some(BlockChange::MakeItem);
        }
        if self.drops.add.is_some_and(|add| ui.clicked(add)) {
            self.block.drops.push(BlockDrop::default());
            return Some(BlockChange::Shape);
        }
        if let Some(gone) = self.drop_rows.iter().position(|row| row.remove.is_some_and(|remove| ui.clicked(remove))) {
            self.block.drops.remove(gone);
            return Some(BlockChange::Shape);
        }
        if ui.changed(self.liquid) {
            let liquid = LIQUIDS[ui.selected_option(self.liquid).min(LIQUIDS.len() - 1)].0;
            if liquid != self.block.liquid {
                self.block.liquid = liquid;
                return Some(BlockChange::Shape);
            }
        }
        if ui.changed(self.heat) {
            self.block.heat = ui.is_checked(self.heat).then(BlockHeat::default);
            return Some(BlockChange::Shape);
        }
        let mut typed = false;
        let block = &mut self.block;
        if let Some(heat) = block.heat.as_mut() {
            if let Some(celsius) = self.celsius.and_then(|field| changed_number(ui, field)) {
                (heat.celsius, typed) = (celsius.clamp(-273.0, 5000.0), true);
            }
            if let Some(reach) = self.reach.and_then(|field| changed_number(ui, field)) {
                (heat.reach, typed) = (reach.clamp(0.0, MAX_HEAT_REACH), true);
            }
        }
        if self.color.iter().any(|&field| ui.changed(field)) {
            for (at, field) in self.color.iter().enumerate() {
                block.color[at] = ui.text(*field).trim().parse::<f32>().unwrap_or(block.color[at]).clamp(0.0, 1.0);
            }
            typed = true;
        }
        if let Some(seconds) = changed_number(ui, self.hardness) {
            (block.hardness, typed) = (seconds.max(0.0), true);
        }
        if ui.changed(self.unbreakable) {
            (block.unbreakable, typed) = (ui.is_checked(self.unbreakable), true);
        }
        if let Some(sways) = self.sways.filter(|&sways| ui.changed(sways)) {
            (block.sways, typed) = (ui.is_checked(sways), true);
        }
        if let Some(shape) = self.shape.filter(|&shape| ui.changed(shape)) {
            let picked = BlockShape::ALL[ui.selected_option(shape).min(BlockShape::ALL.len() - 1)];
            (block.shape, typed) = (picked, true);
        }
        if let Some(tier) = changed_number(ui, self.tier) {
            (block.tier, typed) = (tier.round().clamp(0.0, 1000.0) as u32, true);
        }
        for (row, drop) in self.drop_rows.iter().zip(&mut block.drops) {
            let counts = row.count.map(|field| changed_number(ui, field));
            if counts.iter().any(Option::is_some) {
                let [low, high] = [0, 1].map(|at| counts[at].map_or(drop.count[at], |count| count.max(0.0) as u32));
                (drop.count, typed) = ([low.min(high), low.max(high)], true);
            }
            if let Some(chance) = changed_number(ui, row.chance) {
                (drop.chance, typed) = (chance.clamp(0.0, 1.0), true);
            }
        }
        if typed {
            self.show_swatch(ui);
        }
        typed.then_some(BlockChange::Typed)
    }

    /// Its colour's swatch where it was clicked, with the colour as the
    /// picker shows it.
    pub fn color_clicked(&self, ui: &Ui) -> Option<(NodeId, PickedColor)> {
        ui.clicked(self.swatch).then(|| (self.swatch, PickedColor::srgb(self.block.color)))
    }

    /// Sets its colour from the colour picker, in its fields and swatch.
    pub fn set_color(&mut self, ui: &mut Ui, color: PickedColor) {
        self.block.color = color.srgb;
        for (field, value) in self.color.into_iter().zip(color.srgb) {
            ui.set_text(field, &number(value));
        }
        self.show_swatch(ui);
    }

    fn show_swatch(&self, ui: &mut Ui) {
        let hex = PickedColor::srgb(self.block.color).hex();
        ui.set_inline_style(self.swatch, &format!("background-color: {hex}"));
    }
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
