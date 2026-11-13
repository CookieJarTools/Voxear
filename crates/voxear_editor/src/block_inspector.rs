//! A Block in the Inspector, laid out as a material's is. Its **Look**: its
//! colour, the material every face is drawn with and those its top and
//! bottom have of their own, its shape (a cube, a cube with tufts of leaves,
//! or a vine), and which liquid it is (a liquid is drawn in its colour
//! alone, so its materials and its shape go). **Breaking**: how long it takes
//! by hand, the kind of tool that breaks it fast, and the tier of that tool
//! that gets its drops. Its **Drops**, each an item, how many and how
//! often. Its **Sounds**, and the **Data** asset a game gives it. A block of
//! parts has its **Use** (whether a player uses it, how long a button stays
//! on, how far a hanging block swings), its **Motion**'s keys, and a block
//! of its own boxes its **Boxes**. Each
//! change is saved as it is made, and every world of blocks listing the
//! block takes it up at once (see `editor::blocks`).

use voxear_terrain::blocks::{
    block::{BlockDef, BlockDrop, BlockHeat, BlockShape, BoxHinge, Liquid, MAX_HEAT_REACH},
    shape::{BlockFacing, BlockMotion, BoxDef, MOST_BOXES, MOST_KEYS, MOST_SWING, MotionEase, MotionKey, UseKind},
};
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

/// A key's row of a block's motion: when, how far turned and moved, and
/// how the way on goes.
struct KeyRow {
    time: NodeId,
    turn: NodeId,
    shift: NodeId,
    ease: NodeId,
    remove: Option<NodeId>,
}

/// A box's row of a block of its own boxes: from and to, in sixteenths, and
/// whether it moves.
struct BoxRow {
    from: [NodeId; 3],
    to: [NodeId; 3],
    moves: NodeId,
    remove: Option<NodeId>,
}

/// A block of parts' fields: whether it is used, a button's time, a hanging
/// block's swing and whether one of its own boxes hangs; its motion's keys;
/// and its own boxes, with what their moving ones turn about.
#[derive(Default)]
struct PartFields {
    usable: Option<NodeId>,
    press: Option<NodeId>,
    swing: Option<NodeId>,
    hangs: Option<NodeId>,
    keys: Option<Collection>,
    key_rows: Vec<KeyRow>,
    boxes: Option<Collection>,
    box_rows: Vec<BoxRow>,
    pivot: Option<[NodeId; 3]>,
    axis: Option<NodeId>,
}

/// A row of three numbers side by side, as a vector's.
fn three(ui: &mut Ui, rows: NodeId, label: &str, values: [f32; 3]) -> [NodeId; 3] {
    let row = prop(ui, rows, label);
    let trio = ui.panel(row, Style::row());
    ui.add_class(trio, "vector");
    values.map(|value| {
        let shown = ui.text_field(trio, &number(value));
        ui.add_class(shown, "value-input");
        ui.add_class(shown, "number-input");
        shown
    })
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
    /// How far up the order of blocks that meet it lies; none for a liquid.
    overlap: Option<NodeId>,
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
    parts: PartFields,
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
        let overlap = block.liquid.is_none().then(|| {
            let field = number_field(ui, rows, "Overlap", f32::from(block.overlap));
            let tip = "Where it lies beside a block with a lower Overlap, in one plane, its picture creeps over that \
                       block's edge: 2 for grass, 1 for dirt and 0 for rock has grass over both and dirt over rock";
            if let Some(row) = ui.parent(field) {
                ui.set_tooltip(row, tip, None);
            }
            field
        });
        let shape = block.liquid.is_none().then(|| {
            let row = prop(ui, rows, "Shape");
            let tip = "Cube: a block as most are. Bushy: a cube with tufts of leaves reaching out past it. Vine: thin \
                       sheets on the blocks beside it, or hanging; walked through. The rest are blocks of parts: slabs, \
                       stairs, fences, walls and panes that join what is beside them, doors, trap doors, gates, levers \
                       and buttons a player uses, chains and ropes that hang, or Boxes of the block's own";
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

        let parts = if block.liquid.is_none() { part_fields(ui, root, &block) } else { PartFields::default() };

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
                overlap,
                shape,
                tier,
                heat,
                celsius,
                reach,
                drops,
                drop_rows,
                make_item,
                slots,
                parts,
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
        if let Some(shape) = self.shape.filter(|&shape| ui.changed(shape)) {
            let picked = BlockShape::ALL[ui.selected_option(shape).min(BlockShape::ALL.len() - 1)];
            if picked != self.block.shape {
                self.block.shape = picked;
                // A block of its own boxes starts as one, a box half a block high, to shape from.
                if picked == BlockShape::Boxes && self.block.boxes.is_empty() {
                    self.block.boxes.push(BoxDef { from: [2, 0, 2], to: [14, 8, 14], moves: false });
                }
                return Some(BlockChange::Shape);
            }
        }
        if let Some(change) = self.update_parts(ui) {
            return Some(change);
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
        if let Some(overlap) = self.overlap.and_then(|overlap| changed_number(ui, overlap)) {
            (block.overlap, typed) = (overlap.round().clamp(0.0, 255.0) as u8, true);
        }
        if let Some(sways) = self.sways.filter(|&sways| ui.changed(sways)) {
            (block.sways, typed) = (ui.is_checked(sways), true);
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

    /// What was done to its parts' fields this frame, the block changed to
    /// match.
    fn update_parts(&mut self, ui: &mut Ui) -> Option<BlockChange> {
        let parts = &self.parts;
        let block = &mut self.block;
        if let Some(keys) = &parts.keys {
            if keys.add.is_some_and(|add| ui.clicked(add)) {
                let mut motion = shown_motion(block);
                let last = motion.keys.last().copied().unwrap_or_default();
                motion.keys.push(MotionKey { time: last.time + 0.1, ..last });
                block.motion = Some(motion);
                return Some(BlockChange::Shape);
            }
            if let Some(gone) = parts.key_rows.iter().position(|row| row.remove.is_some_and(|remove| ui.clicked(remove))) {
                let mut motion = shown_motion(block);
                motion.keys.remove(gone);
                block.motion = Some(motion);
                return Some(BlockChange::Shape);
            }
        }
        if let Some(boxes) = &parts.boxes {
            if boxes.add.is_some_and(|add| ui.clicked(add)) && block.boxes.len() < MOST_BOXES {
                block.boxes.push(BoxDef { from: [6, 0, 6], to: [10, 4, 10], moves: false });
                return Some(BlockChange::Shape);
            }
            if let Some(gone) = parts.box_rows.iter().position(|row| row.remove.is_some_and(|remove| ui.clicked(remove))) {
                block.boxes.remove(gone);
                return Some(BlockChange::Shape);
            }
        }
        let mut typed = false;
        if let Some(usable) = parts.usable.filter(|&usable| ui.changed(usable)) {
            (block.usable, typed) = (ui.is_checked(usable), true);
        }
        if let Some(seconds) = parts.press.and_then(|press| changed_number(ui, press)) {
            (block.press_time, typed) = (Some(seconds.clamp(0.05, 60.0)), true);
        }
        if let Some(degrees) = parts.swing.and_then(|swing| changed_number(ui, swing)) {
            (block.swing, typed) = (Some(degrees.clamp(0.0, MOST_SWING)), true);
        }
        if let Some(hangs) = parts.hangs.filter(|&hangs| ui.changed(hangs)) {
            block.hangs = ui.is_checked(hangs);
            return Some(BlockChange::Shape);
        }
        // A key changed makes the motion the block's own, from what it showed.
        let mut motion = shown_motion(block);
        let mut moved = false;
        for (row, key) in parts.key_rows.iter().zip(&mut motion.keys) {
            if let Some(time) = changed_number(ui, row.time) {
                (key.time, moved) = (time, true);
            }
            if let Some(turn) = changed_number(ui, row.turn) {
                (key.turn, moved) = (turn, true);
            }
            if let Some(shift) = changed_number(ui, row.shift) {
                (key.shift, moved) = (shift, true);
            }
            if ui.changed(row.ease) {
                (key.ease, moved) = (MotionEase::ALL[ui.selected_option(row.ease).min(2)], true);
            }
        }
        if moved {
            block.motion = Some(motion.tidied());
            typed = true;
        }
        for (row, part) in parts.box_rows.iter().zip(&mut block.boxes) {
            for axis in 0..3 {
                if let Some(from) = changed_number(ui, row.from[axis]) {
                    (part.from[axis], typed) = (from.round().clamp(0.0, 16.0) as u8, true);
                }
                if let Some(to) = changed_number(ui, row.to[axis]) {
                    (part.to[axis], typed) = (to.round().clamp(0.0, 16.0) as u8, true);
                }
            }
            if ui.changed(row.moves) {
                (part.moves, typed) = (ui.is_checked(row.moves), true);
            }
        }
        if let Some(pivot) = parts.pivot {
            let mut hinge = block.hinge.unwrap_or_default();
            for axis in 0..3 {
                if let Some(at) = changed_number(ui, pivot[axis]) {
                    (hinge.pivot[axis], typed) = (at.clamp(0.0, 15.5), true);
                    block.hinge = Some(hinge);
                }
            }
        }
        if let Some(axis) = parts.axis.filter(|&axis| ui.changed(axis)) {
            let mut hinge: BoxHinge = block.hinge.unwrap_or_default();
            hinge.axis = BlockFacing::ALL[ui.selected_option(axis).min(5)];
            (block.hinge, typed) = (Some(hinge), true);
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

/// The motion a block shows: its own, or its shape's where it has none.
fn shown_motion(block: &BlockDef) -> BlockMotion {
    block.motion.clone().or_else(|| BlockMotion::default_for(block.shape)).unwrap_or_else(|| BlockMotion {
        keys: vec![MotionKey::default(), MotionKey { time: 0.3, turn: 90.0, ..MotionKey::default() }],
    })
}

/// The fields of `block` as a block of parts, under `root`: **Use** for one
/// a player uses or that hangs, **Motion** for one that moves, and
/// **Boxes** for one of its own boxes. None for a cube.
fn part_fields(ui: &mut Ui, root: NodeId, block: &BlockDef) -> PartFields {
    let mut fields = PartFields::default();
    let shape = block.shape;
    let looks = block.part_look();
    let opens = !matches!(UseKind::of(shape, false, false), UseKind::None);
    // Use, where it has a row: a door, a gate and a lever are used whatever they say, so show none of their own.
    let has_use = !opens || shape == BlockShape::Button || shape == BlockShape::Boxes || looks.hangs;
    if (shape.is_part() || block.usable) && has_use {
        let rows = part(ui, root, "Use", "settings");
        if !opens {
            let row = prop(ui, rows, "Usable");
            ui.set_tooltip(row, "A player can use it, and scripts hear it was used", None);
            fields.usable = Some(ui.toggle(row, "", block.usable));
        }
        if shape == BlockShape::Button {
            fields.press = Some(number_field(ui, rows, "Press Time", looks.press_time));
        }
        if shape == BlockShape::Boxes {
            let row = prop(ui, rows, "Hangs");
            fields.hangs = Some(ui.toggle(row, "", block.hangs));
        }
        if looks.hangs {
            let field = number_field(ui, rows, "Swing", looks.swing);
            if let Some(row) = ui.parent(field) {
                ui.set_tooltip(row, "The most it swings in the wind and as things brush past it, in degrees", None);
            }
            fields.swing = Some(field);
        }
    }
    if shape == BlockShape::Boxes {
        let rows = part(ui, root, "Boxes", "box");
        let boxes = collection::collection(ui, rows, block.boxes.len(), true, Some("Add Box"));
        ui.set_reorderable(boxes.items, false);
        ui.set_text(boxes.empty, "No boxes");
        for (place, def) in block.boxes.iter().enumerate() {
            let item = collection::fixed_item(ui, boxes.items, place);
            let from = three(ui, item.body, "From", def.from.map(f32::from));
            let to = three(ui, item.body, "To", def.to.map(f32::from));
            let row = prop(ui, item.body, "Moves");
            let moves = ui.toggle(row, "", def.moves);
            fields.box_rows.push(BoxRow { from, to, moves, remove: item.remove });
        }
        fields.boxes = Some(boxes);
        let hinge = block.hinge.unwrap_or_default();
        fields.pivot = Some(three(ui, rows, "Pivot", hinge.pivot));
        let row = prop(ui, rows, "Axis");
        let picked = BlockFacing::ALL.iter().position(|way| *way == hinge.axis).unwrap_or(4);
        let axis = ui.dropdown(row, &BlockFacing::ALL.map(BlockFacing::name), picked);
        ui.add_class(axis, "value-choice");
        fields.axis = Some(axis);
    }
    let moves = looks.motion.is_some() || shape == BlockShape::Boxes && block.boxes.iter().any(|part| part.moves);
    if moves {
        let rows = part(ui, root, "Motion", "play");
        let motion = shown_motion(block);
        let room = motion.keys.len() < MOST_KEYS;
        let keys = collection::collection(ui, rows, motion.keys.len(), true, room.then_some("Add Key"));
        ui.set_reorderable(keys.items, false);
        ui.set_text(keys.empty, "No keys");
        for (place, key) in motion.keys.iter().enumerate() {
            let item = collection::fixed_item(ui, keys.items, place);
            let time = number_field(ui, item.body, "Time", key.time);
            let turn = number_field(ui, item.body, "Turn", key.turn);
            let shift = number_field(ui, item.body, "Move", key.shift);
            let row = prop(ui, item.body, "Ease");
            let picked = MotionEase::ALL.iter().position(|ease| *ease == key.ease).unwrap_or(1);
            let ease = ui.dropdown(row, &MotionEase::ALL.map(MotionEase::name), picked);
            ui.add_class(ease, "value-choice");
            fields.key_rows.push(KeyRow { time, turn, shift, ease, remove: item.remove });
        }
        fields.keys = Some(keys);
    }
    fields
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
