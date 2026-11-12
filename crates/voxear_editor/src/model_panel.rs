//! The Model window: a box model, to build by hand. It opens from a double
//! click on a `.boxmodel` in the Project.
//!
//! On the left are the model's bones, each with its boxes and the bones
//! under it, to pick from and to drag onto one another. Beside them the
//! model stands on a floor of blocks (see `model_view`): a click picks the
//! box under it, and what is picked has handles to move it and, for a box,
//! each of its faces. The bar over both adds a bone or a box, copies what
//! is picked, makes its mirror across the body, or takes it out.
//!
//! The bone or box picked shows its numbers in the Inspector (see
//! `box_part_inspector`). Every change makes a step to undo, which Ctrl+Z
//! and Ctrl+Y reach while the window is the one last pressed in; changes
//! close together to one row, and the whole of one drag, are one step. The
//! file is saved once nothing is held: when a drag ends, not at each step
//! of it.

use std::{
    cell::RefCell,
    rc::Rc,
    time::{Duration, Instant},
};

use voxear_core::math::Vec3;
use voxear_render::{BoxModel, BoxSkin, Face, Renderer};
use voxear_ui::{ColorPicker, DropPlace, NodeId, PickedColor, Style, TreeItems, Ui};

use crate::{
    box_part_inspector::BoxPartEdit,
    model_sheet::SheetView,
    model_view::{Handle, ModelView, Picked, ViewEvent, ViewMode},
};

/// Changes to the same thing closer together than this are one step to undo, as an Obj's are.
const MERGE_WITHIN: Duration = Duration::from_millis(800);
/// The most steps kept to undo.
const MOST_STEPS: usize = 200;
/// The size a new box starts at, in pixels a side.
const NEW_BOX: [u32; 3] = [4, 4, 4];
/// A bone's number in the tree is its place among the bones times this; a
/// box's is its bone's plus its own place and one.
const BONE: u64 = 4096;

/// The most of a sheet's colours the bar offers to paint with again.
const SWATCHES: usize = 12;

/// What a change is to, for changes close together to merge into one step.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EditKey {
    /// A row of the Inspector, for one part.
    Row(Picked, u8),
    /// A drag from a handle.
    Drag,
    /// A stroke of paint.
    Stroke,
}

/// What the window is for at the moment: shaping the model, or painting it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    #[default]
    Shape,
    Paint,
}

/// What a stroke does to the pixels it passes over.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tool {
    /// Colours them.
    #[default]
    Pencil,
    /// Makes holes of them.
    Eraser,
    /// Colours every pixel of the face joined to the one pressed by pixels of its colour.
    Fill,
    /// Takes the colour of the one pressed to paint with.
    Pick,
}

impl Tool {
    const ALL: [Tool; 4] = [Tool::Pencil, Tool::Eraser, Tool::Fill, Tool::Pick];

    fn label(self) -> &'static str {
        match self {
            Tool::Pencil => "Pencil",
            Tool::Eraser => "Eraser",
            Tool::Fill => "Fill",
            Tool::Pick => "Pick",
        }
    }
}

/// The models an open one has been, to go back to and forward again.
#[derive(Default)]
struct History {
    done: Vec<(BoxModel, Option<Picked>)>,
    undone: Vec<(BoxModel, Option<Picked>)>,
    last: Option<(EditKey, Instant)>,
}

impl History {
    /// Notes the model as it was `before` a change to `key`, unless the
    /// change is one more to what was changed a moment ago.
    fn record(&mut self, before: &BoxModel, picked: Option<Picked>, key: Option<EditKey>) {
        let merges =
            matches!((key, self.last), (Some(key), Some((last, at))) if key == last && at.elapsed() < MERGE_WITHIN);
        if !merges {
            self.done.push((before.clone(), picked));
            if self.done.len() > MOST_STEPS {
                self.done.remove(0);
            }
        }
        self.undone.clear();
        self.last = key.map(|key| (key, Instant::now()));
    }

    fn seal(&mut self) {
        self.last = None;
    }
}

/// The model that is open.
struct Open {
    path: String,
    model: BoxModel,
    history: History,
    picked: Option<Picked>,
    /// Changed since it was last saved.
    unsaved: bool,
    /// The model as a drag from a handle found it, which the drag changes.
    held: Option<BoxModel>,
    /// The view and the tree show an older model.
    stale: bool,
    /// The views show older pixels, of a model whose shape they have.
    repainted: bool,
}

/// What the panel asks of the editor.
#[derive(Clone, Debug, PartialEq)]
pub enum ModelEvent {
    /// Save the model at this path as it now is.
    Save(String, BoxModel),
    /// Show this bone or box in the Inspector, or with none the model itself.
    Inspect(Option<Picked>),
}

/// A row of the tree: its number, the row it is in, its name and its icon.
#[derive(Clone, Debug, PartialEq)]
struct TreeRow {
    id: u64,
    parent: Option<u64>,
    name: String,
    icon: &'static str,
}

/// The tree's rows, shared with the tree itself.
#[derive(Default)]
struct Rows(Vec<TreeRow>);

struct ModelTree(Rc<RefCell<Rows>>);

impl TreeItems for ModelTree {
    fn children(&self, parent: Option<u64>) -> Vec<u64> {
        self.0.borrow().0.iter().filter(|row| row.parent == parent).map(|row| row.id).collect()
    }

    fn make(&mut self, ui: &mut Ui, row: NodeId) {
        ui.add_class(row, "folder-row");
        let icon = ui.icon(row, "cube");
        ui.add_class(icon, "row-icon");
        let name = ui.add_label(row, "");
        ui.add_class(name, "row-name");
    }

    fn bind(&mut self, ui: &mut Ui, row: NodeId, item: u64) {
        let rows = self.0.borrow();
        let Some(shown) = rows.0.iter().find(|shown| shown.id == item) else { return };
        let parts = ui.children(row);
        ui.set_icon(parts[0], shown.icon);
        ui.set_text(parts[1], &shown.name);
    }
}

/// A part's number in the tree, and the part a number is.
fn tree_id(part: Picked) -> u64 {
    match part {
        Picked::Bone(bone) => bone as u64 * BONE,
        Picked::Box(bone, index) => bone as u64 * BONE + index as u64 + 1,
    }
}

fn tree_part(id: u64) -> Picked {
    let bone = (id / BONE) as usize;
    match id % BONE {
        0 => Picked::Bone(bone),
        index => Picked::Box(bone, index as usize - 1),
    }
}

fn tree_rows(model: &BoxModel) -> Vec<TreeRow> {
    let mut rows = Vec::new();
    for (at, bone) in model.bones.iter().enumerate() {
        let id = tree_id(Picked::Bone(at));
        let parent = bone.parent.map(|parent| tree_id(Picked::Bone(parent)));
        rows.push(TreeRow { id, parent, name: bone.name.clone(), icon: "branch" });
        for index in 0..bone.boxes.len() {
            let name = format!("Box {}", index + 1);
            rows.push(TreeRow { id: tree_id(Picked::Box(at, index)), parent: Some(id), name, icon: "cube" });
        }
    }
    rows
}

pub struct ModelPanel {
    /// Shown while no model is open, and the rest while one is.
    hint: NodeId,
    body: NodeId,
    name: NodeId,
    add_bone: NodeId,
    add_box: NodeId,
    duplicate: NodeId,
    mirror: NodeId,
    delete: NodeId,
    pivot_only: NodeId,
    frame: NodeId,
    summary: NodeId,
    /// Shape or Paint, and the buttons of each, shown while it is the mode.
    modes: NodeId,
    shape_bar: NodeId,
    paint_bar: NodeId,
    tools: NodeId,
    /// The colour painted with: its swatch, which opens the picker, the
    /// picker in its frame, and the sheet's own colours to take again.
    swatch: NodeId,
    picker_frame: NodeId,
    picker: ColorPicker,
    swatches: NodeId,
    offered: Vec<[u8; 4]>,
    color: [u8; 4],
    /// The colour the swatch and the picker last showed.
    color_shown: Option<[u8; 4]>,
    symmetry: NodeId,
    lay_out: NodeId,
    mode: Mode,
    tool: Tool,
    tree: NodeId,
    rows: Rc<RefCell<Rows>>,
    view: ModelView,
    sheet: SheetView,
    open: Option<Open>,
    /// The panel was the last one pressed in, so the keys are its model's.
    active: bool,
    /// Whether Pivot Only was on at the last frame: moving a bone then moves its pivot alone.
    pivot_alone: bool,
    showing: Option<bool>,
    pending: Vec<ModelEvent>,
}

impl ModelPanel {
    pub fn new(ui: &mut Ui, root: NodeId) -> Self {
        let hint = ui.add_label(root, "Double-click a Box Model in the Project to build it.");
        ui.add_class(hint, "placeholder");
        let body = ui.panel(root, Style::column());
        ui.add_class(body, "fill");
        let bar = ui.panel(body, Style::row());
        ui.add_class(bar, "console-toolbar");
        let icon = ui.icon(bar, "box");
        ui.add_class(icon, "generator-icon");
        let name = ui.add_label(bar, "");
        ui.add_class(name, "generator-name");
        let modes = ui.segmented(bar);
        ui.add_segment(modes, "Shape", None);
        ui.add_segment(modes, "Paint", None);
        let group = |ui: &mut Ui| {
            let group = ui.panel(bar, Style::row());
            ui.add_class(group, "model-bar-group");
            group
        };
        let button = |ui: &mut Ui, parent: NodeId, text: &str, tip: &str| {
            let button = ui.button(parent, text);
            ui.add_class(button, "console-button");
            ui.set_tooltip(button, tip, None);
            button
        };
        let shape_bar = group(ui);
        let add_bone = button(ui, shape_bar, "Add Bone", "A bone under the one picked, or at the top");
        let add_box = button(ui, shape_bar, "Add Box", "A box on the bone picked");
        let duplicate = button(ui, shape_bar, "Duplicate", "A copy of what is picked");
        let mirror = button(ui, shape_bar, "Mirror", "A copy of what is picked on the other side of the body");
        let delete = button(ui, shape_bar, "Delete", "Takes out what is picked");
        let tip = "Moving a bone moves its pivot and leaves its boxes where they are";
        let pivot_only = button(ui, shape_bar, "Pivot Only", tip);

        let paint_bar = group(ui);
        let tools = ui.segmented(paint_bar);
        for tool in Tool::ALL {
            ui.add_segment(tools, tool.label(), None);
        }
        let swatch = ui.panel(paint_bar, Style::default());
        ui.add_class(swatch, "model-swatch");
        ui.add_class(swatch, "model-swatch-own");
        ui.set_tooltip(swatch, "The colour painted with", None);
        let swatches = ui.panel(paint_bar, Style::row());
        ui.add_class(swatches, "model-bar-group");
        let symmetry = button(ui, paint_bar, "Symmetry", "A stroke on the model paints the other side of the body too");
        let tip = "Gives every unfolded box room of its own on the sheet, with its pixels";
        let lay_out = button(ui, paint_bar, "Lay Out", tip);
        ui.set_visible(paint_bar, false);

        let frame = button(ui, bar, "Frame", "Looks at the whole model");
        let summary = ui.add_label(bar, "");
        ui.add_class(summary, "profiler-summary");
        let line = ui.panel(body, Style::default());
        ui.add_class(line, "divider");

        let split = ui.panel(body, Style::default());
        ui.add_class(split, "fill");
        ui.add_class(split, "generator-split");
        let side = ui.panel(split, Style::column());
        ui.add_class(side, "model-tree");
        let rows = Rc::new(RefCell::new(Rows::default()));
        let tree = ui.tree_view(side, 24.0, ModelTree(rows.clone()));
        ui.add_class(tree, "fill");
        ui.set_rows_draggable(tree, true);
        let view = ModelView::new(ui, split);
        let sheet = SheetView::new(ui, split);
        ui.set_visible(sheet.frame_node(), false);
        // Over the view's corner, while the colour is being picked.
        let picker_frame = ui.panel(split, Style::default());
        ui.add_class(picker_frame, "model-picker");
        let picker = ColorPicker::new(ui, picker_frame);
        ui.set_visible(picker_frame, false);
        ui.set_visible(body, false);
        Self {
            modes,
            shape_bar,
            paint_bar,
            tools,
            swatch,
            picker_frame,
            picker,
            swatches,
            offered: Vec::new(),
            color: [226, 84, 84, 255],
            color_shown: None,
            symmetry,
            lay_out,
            mode: Mode::Shape,
            tool: Tool::Pencil,
            sheet,
            hint,
            body,
            name,
            add_bone,
            add_box,
            duplicate,
            mirror,
            delete,
            pivot_only,
            frame,
            summary,
            tree,
            rows,
            view,
            open: None,
            active: false,
            pivot_alone: false,
            showing: None,
            pending: Vec::new(),
        }
    }

    /// Opens the model at `path`, as `model` reads it: the whole of it in
    /// view, nothing picked, and nothing to undo.
    pub fn open(&mut self, ui: &mut Ui, path: &str, model: BoxModel) {
        self.show_name(ui, path);
        self.view.frame_model(&model);
        self.open = Some(Open {
            path: path.to_string(),
            model,
            history: History::default(),
            picked: None,
            unsaved: false,
            held: None,
            stale: true,
            repainted: true,
        });
        self.active = true;
    }

    fn show_name(&mut self, ui: &mut Ui, path: &str) {
        let file = path.rsplit('/').next().unwrap_or(path);
        ui.set_text(self.name, file.strip_suffix(".boxmodel").unwrap_or(file));
        ui.set_tooltip(self.name, path, None);
    }

    /// The open model and its file, when it has a change not yet saved:
    /// taken as saved.
    pub fn take_unsaved(&mut self) -> Option<(String, BoxModel)> {
        let open = self.open.as_mut().filter(|open| open.unsaved)?;
        open.unsaved = false;
        Some((open.path.clone(), open.model.clone()))
    }

    /// Closes the open model, as when its file goes.
    pub fn close(&mut self, renderer: &mut Renderer) {
        self.open = None;
        self.view.clear(renderer);
        self.sheet.clear(renderer);
    }

    /// The open model's file, by its path in the project.
    pub fn path(&self) -> Option<&str> {
        self.open.as_ref().map(|open| open.path.as_str())
    }

    /// The open model, as it now is.
    pub fn model(&self) -> Option<&BoxModel> {
        self.open.as_ref().map(|open| &open.model)
    }

    /// The bone or box picked.
    pub fn picked(&self) -> Option<Picked> {
        self.open.as_ref().and_then(|open| open.picked)
    }

    /// The open model's file renamed or moved to `path`.
    pub fn moved_to(&mut self, ui: &mut Ui, path: &str) {
        if let Some(open) = self.open.as_mut() {
            open.path = path.to_string();
            self.show_name(ui, path);
        }
    }

    /// The open model's file changed outside the editor: it is read again
    /// as `model`, a step to undo.
    pub fn reread(&mut self, model: BoxModel) {
        self.change(model, None);
        if let Some(open) = self.open.as_mut() {
            open.unsaved = false;
        }
    }

    pub fn set_active(&mut self, active: bool) {
        self.active = active;
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn set_dark(&mut self, dark: bool) {
        self.view.set_dark(dark);
    }

    /// The view of the model, for a test to find where things are drawn.
    pub fn view(&self) -> &ModelView {
        &self.view
    }

    /// The tree of bones and boxes.
    pub fn tree(&self) -> NodeId {
        self.tree
    }

    /// The flat view of the model's sheet, which shows while painting.
    pub fn sheet_view(&self) -> &SheetView {
        &self.sheet
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    /// Shapes the model, or paints it.
    pub fn set_mode(&mut self, ui: &mut Ui, mode: Mode) {
        ui.set_segment(self.modes, mode as usize);
        self.mode = mode;
    }

    pub fn tool(&self) -> Tool {
        self.tool
    }

    pub fn set_tool(&mut self, ui: &mut Ui, tool: Tool) {
        ui.set_segment(self.tools, tool as usize);
        self.tool = tool;
    }

    /// The colour painted with: red, green, blue and how solid.
    pub fn color(&self) -> [u8; 4] {
        self.color
    }

    pub fn set_color(&mut self, color: [u8; 4]) {
        self.color = [color[0], color[1], color[2], 255];
    }

    /// Whether a stroke on the model paints the other side of the body too.
    pub fn set_symmetry(&mut self, ui: &mut Ui, on: bool) {
        ui.set_checked(self.symmetry, on);
    }

    /// Whether moving a bone moves its pivot alone.
    pub fn pivot_only(&self, ui: &Ui) -> bool {
        ui.is_checked(self.pivot_only)
    }

    /// Picks a bone or box, or with none, nothing: outlined in the view,
    /// its row lit in the tree, and shown in the Inspector.
    pub fn pick(&mut self, picked: Option<Picked>) {
        let Some(open) = self.open.as_mut() else { return };
        let picked = picked.filter(|picked| has(&open.model, *picked));
        if open.picked != picked {
            (open.picked, open.stale) = (picked, true);
            self.pending.push(ModelEvent::Inspect(picked));
        }
    }

    /// The open model becomes `model`, as a step to undo.
    pub fn change(&mut self, model: BoxModel, key: Option<EditKey>) {
        let Some(open) = self.open.as_mut() else { return };
        if model == open.model {
            return;
        }
        open.history.record(&open.model, open.picked, key);
        // Pixels alone changed on a sheet of the same size: the shape the views have still stands.
        let reshaped = model.bones != open.model.bones
            || model.pixels_per_block != open.model.pixels_per_block
            || (model.sheet.width, model.sheet.height) != (open.model.sheet.width, open.model.sheet.height);
        (open.stale, open.repainted) = (open.stale || reshaped, open.repainted || model.sheet != open.model.sheet);
        (open.model, open.unsaved) = (model, true);
        if open.picked.is_some_and(|picked| !has(&open.model, picked)) {
            open.picked = None;
            self.pending.push(ModelEvent::Inspect(None));
        }
    }

    /// Goes back a step, or forward again. False with no step to take.
    pub fn undo(&mut self) -> bool {
        self.step(false)
    }

    pub fn redo(&mut self) -> bool {
        self.step(true)
    }

    fn step(&mut self, forward: bool) -> bool {
        let Some(open) = self.open.as_mut() else { return false };
        let history = &mut open.history;
        let (from, to) =
            if forward { (&mut history.undone, &mut history.done) } else { (&mut history.done, &mut history.undone) };
        let Some((model, picked)) = from.pop() else { return false };
        to.push((open.model.clone(), open.picked));
        history.last = None;
        (open.model, open.unsaved, open.stale, open.repainted, open.held) = (model, true, true, true, None);
        let picked = picked.filter(|picked| has(&open.model, *picked));
        if open.picked != picked {
            open.picked = picked;
            self.pending.push(ModelEvent::Inspect(picked));
        }
        true
    }

    /// Adds a bone under the one picked, or under a picked box's, or at
    /// the top with nothing picked, and picks it.
    pub fn add_bone(&mut self) {
        let Some(open) = self.open.as_ref() else { return };
        let mut model = open.model.clone();
        let parent = open.picked.map(Picked::bone);
        let name = model.unique_name(if model.bones.is_empty() { "Body" } else { "Bone" });
        model.add_bone(&name, parent, Vec3::ZERO);
        // Under its parent's own, so every parent is before its children.
        let made = model.bones.len() - 1;
        let made = model.set_parent(made, parent).unwrap_or(made);
        self.change(model, None);
        self.pick(Some(Picked::Bone(made)));
    }

    /// Adds a box to the bone picked, or to a picked box's, with room of
    /// its own on the sheet, and picks it. A model with no bone gets one.
    pub fn add_box(&mut self) {
        let Some(open) = self.open.as_ref() else { return };
        let mut model = open.model.clone();
        let bone = match open.picked.map(Picked::bone) {
            Some(bone) => bone,
            None if model.bones.is_empty() => model.add_bone("Body", None, Vec3::ZERO),
            None => 0,
        };
        let index = model.add_box_with_room(bone, NEW_BOX);
        self.change(model, None);
        self.pick(Some(Picked::Box(bone, index)));
    }

    /// Takes out what is picked: a box, or a bone with all under it.
    pub fn delete_picked(&mut self) {
        let Some((open, picked)) = self.open.as_ref().and_then(|open| Some((open, open.picked?))) else { return };
        let mut model = open.model.clone();
        match picked {
            Picked::Bone(bone) => model.remove_bone(bone),
            Picked::Box(bone, index) => drop(model.bones[bone].boxes.remove(index)),
        }
        self.change(model, None);
        self.pick(None);
    }

    /// Copies what is picked beside it, and picks the copy.
    pub fn duplicate_picked(&mut self) {
        self.copy_picked(false);
    }

    /// Copies what is picked to the other side of the body, and picks the
    /// copy: a bone with all under it, or a box on its own bone.
    pub fn mirror_picked(&mut self) {
        self.copy_picked(true);
    }

    fn copy_picked(&mut self, mirrored: bool) {
        let Some((open, picked)) = self.open.as_ref().and_then(|open| Some((open, open.picked?))) else { return };
        let mut model = open.model.clone();
        let made = match picked {
            Picked::Bone(bone) if mirrored => Picked::Bone(model.mirror_bone(bone)),
            Picked::Bone(bone) => Picked::Bone(model.duplicate_bone(bone)),
            Picked::Box(bone, index) => {
                let copied = &model.bones[bone].boxes[index];
                let copy = if mirrored { BoxModel::mirrored_box(copied) } else { copied.clone() };
                model.bones[bone].boxes.push(copy);
                Picked::Box(bone, model.bones[bone].boxes.len() - 1)
            }
        };
        self.change(model, None);
        self.pick(Some(made));
    }

    /// Looks at the whole model.
    pub fn frame(&mut self) {
        if let Some(open) = self.open.as_ref() {
            self.view.frame_model(&open.model);
        }
    }

    /// Takes a change typed in the Inspector's rows into the part picked.
    pub fn edit(&mut self, part: Picked, edit: BoxPartEdit) {
        let Some(open) = self.open.as_ref() else { return };
        let mut model = open.model.clone();
        if !has(&model, part) {
            return;
        }
        let key = EditKey::Row(part, edit.row());
        match (edit, part) {
            (BoxPartEdit::Name(name), Picked::Bone(at)) => {
                // No two bones of a name.
                model.bones[at].name.clear();
                model.bones[at].name = model.unique_name(&name);
            }
            (BoxPartEdit::Pivot(pivot), Picked::Bone(at)) => match self.pivot_alone {
                true => model.move_pivot(at, pivot),
                false => model.bones[at].pivot = pivot,
            },
            (BoxPartEdit::BoneRotation(rotation), Picked::Bone(at)) => model.bones[at].rotation = rotation,
            (edit, Picked::Box(at, index)) => {
                let model_box = &mut model.bones[at].boxes[index];
                match edit {
                    BoxPartEdit::Position(from) => model_box.from = from,
                    BoxPartEdit::Size(size) => model_box.size = size,
                    BoxPartEdit::BoxRotation(rotation) => model_box.rotation = rotation,
                    BoxPartEdit::Inflate(inflate) => model_box.inflate = inflate,
                    BoxPartEdit::Sheet(at) => {
                        if let BoxSkin::Unfolded { mirror, .. } = model_box.skin {
                            model_box.skin = BoxSkin::Unfolded { at, mirror };
                        }
                    }
                    BoxPartEdit::Mirror(mirror) => {
                        if let BoxSkin::Unfolded { at, .. } = model_box.skin {
                            model_box.skin = BoxSkin::Unfolded { at, mirror };
                        }
                    }
                    _ => return,
                }
            }
            _ => return,
        }
        self.change(model, Some(key));
    }

    /// The panel's frame: the bar's buttons, the tree, the view, and what
    /// they ask of the editor. `scale` is the window's, for how fine the
    /// view is drawn.
    pub fn update(&mut self, ui: &mut Ui, shown: bool, renderer: &mut Renderer, scale: f32) -> Vec<ModelEvent> {
        let open = self.open.is_some();
        if self.showing != Some(open) {
            self.showing = Some(open);
            ui.set_visible(self.hint, !open);
            ui.set_visible(self.body, open);
        }
        if !open {
            return std::mem::take(&mut self.pending);
        }
        if shown {
            self.update_bar(ui);
            self.update_paint_bar(ui);
            self.update_tree(ui);
            self.update_view(ui, renderer, scale);
            self.show(ui, renderer);
        }
        let mut events = std::mem::take(&mut self.pending);
        // Saved once nothing is held: when a drag ends, not at each step of it.
        let held = ui.holds_press() || self.view.is_dragging() || self.sheet.is_painting();
        if let Some(open) = self.open.as_mut().filter(|open| open.unsaved && !held) {
            open.unsaved = false;
            events.push(ModelEvent::Save(open.path.clone(), open.model.clone()));
        }
        events
    }

    /// The mode, and while painting the tool, the colour and its picker,
    /// the sheet's colours to take again, Symmetry and Lay Out.
    fn update_paint_bar(&mut self, ui: &mut Ui) {
        let mode = if ui.segment(self.modes) == 1 { Mode::Paint } else { Mode::Shape };
        let painting = mode == Mode::Paint;
        self.mode = mode;
        if ui.is_visible(self.paint_bar) != painting {
            ui.set_visible(self.paint_bar, painting);
            ui.set_visible(self.shape_bar, !painting);
            ui.set_visible(self.sheet.frame_node(), painting);
            ui.set_visible(self.picker_frame, false);
        }
        if !painting {
            return;
        }
        self.tool = Tool::ALL[ui.segment(self.tools).min(Tool::ALL.len() - 1)];
        if ui.clicked(self.symmetry) {
            let on = !ui.is_checked(self.symmetry);
            ui.set_checked(self.symmetry, on);
        }
        if ui.clicked(self.lay_out)
            && let Some(mut model) = self.open.as_ref().map(|open| open.model.clone())
            && model.lay_out()
        {
            self.change(model, None);
        }
        if ui.clicked(self.swatch) {
            let open = !ui.is_visible(self.picker_frame);
            ui.set_visible(self.picker_frame, open);
        }
        if let Some(picked) = self.picker.update(ui) {
            let [r, g, b] = picked.srgb.map(|channel| (channel.clamp(0.0, 1.0) * 255.0).round() as u8);
            (self.color, self.color_shown) = ([r, g, b, 255], Some([r, g, b, 255]));
            ui.set_inline_style(self.swatch, &format!("background-color: #{r:02x}{g:02x}{b:02x}"));
        }
        // The sheet's own colours, each a press away.
        let offered = self.open.as_ref().map(|open| open.model.sheet.colors(SWATCHES)).unwrap_or_default();
        if offered != self.offered {
            for child in ui.children(self.swatches) {
                ui.remove(child);
            }
            for [r, g, b, _] in &offered {
                let swatch = ui.panel(self.swatches, Style::default());
                ui.add_class(swatch, "model-swatch");
                ui.set_inline_style(swatch, &format!("background-color: #{r:02x}{g:02x}{b:02x}"));
            }
            self.offered = offered;
        }
        let taken = ui.children(self.swatches).into_iter().position(|swatch| ui.clicked(swatch));
        if let Some(color) = taken.and_then(|at| self.offered.get(at)) {
            self.color = *color;
        }
        if self.color_shown != Some(self.color) {
            self.color_shown = Some(self.color);
            let [r, g, b, _] = self.color;
            ui.set_inline_style(self.swatch, &format!("background-color: #{r:02x}{g:02x}{b:02x}"));
            self.picker.set(ui, PickedColor::srgb([r, g, b].map(|channel| f32::from(channel) / 255.0)), false);
        }
    }

    /// One pixel of a stroke: coloured, made a hole, filled from, or its
    /// colour taken, as the tool says.
    fn paint(&mut self, pixel: [u32; 2], within: [u32; 4]) {
        let Some(open) = self.open.as_ref() else { return };
        let mut model = open.model.clone();
        match self.tool {
            Tool::Pencil => model.sheet.set(pixel[0], pixel[1], self.color),
            Tool::Eraser => model.sheet.set(pixel[0], pixel[1], [0; 4]),
            Tool::Fill => drop(model.sheet.flood(within, pixel, self.color)),
            Tool::Pick => {
                let under = model.sheet.get(pixel[0], pixel[1]);
                if under[3] >= 128 {
                    self.set_color(under);
                }
                return;
            }
        }
        self.change(model, Some(EditKey::Stroke));
    }

    fn update_bar(&mut self, ui: &mut Ui) {
        if ui.clicked(self.pivot_only) {
            let on = !ui.is_checked(self.pivot_only);
            ui.set_checked(self.pivot_only, on);
        }
        self.pivot_alone = ui.is_checked(self.pivot_only);
        if ui.clicked(self.add_bone) {
            self.add_bone();
        }
        if ui.clicked(self.add_box) {
            self.add_box();
        }
        if ui.clicked(self.duplicate) {
            self.duplicate_picked();
        }
        if ui.clicked(self.mirror) {
            self.mirror_picked();
        }
        if ui.clicked(self.delete) {
            self.delete_picked();
        }
        if ui.clicked(self.frame) {
            self.frame();
        }
        let picked = self.picked().is_some();
        for button in [self.duplicate, self.mirror, self.delete] {
            if ui.is_enabled(button) != picked {
                ui.set_enabled(button, picked);
            }
        }
    }

    /// A row picked in the tree, or dragged onto another: a bone under
    /// another bone, or to the top; a box to another bone.
    fn update_tree(&mut self, ui: &mut Ui) {
        if ui.selection_changed(self.tree) {
            let picked = ui.selected_items(self.tree).first().map(|&id| tree_part(id));
            if picked != self.picked() {
                self.pick(picked);
            }
        }
        let Some((dragged, onto)) = ui.dropped_item(self.tree) else { return };
        let Some(open) = self.open.as_ref() else { return };
        let mut model = open.model.clone();
        // Onto a bone is under it; before or after a bone is beside it; a box stands for its bone.
        let target = onto.map(|(id, place)| match (tree_part(id), place) {
            (Picked::Bone(bone), DropPlace::Onto) | (Picked::Box(bone, _), _) => Some(bone),
            (Picked::Bone(bone), _) => model.bones[bone].parent,
        });
        let made = match (tree_part(dragged), target) {
            (Picked::Bone(bone), Some(parent)) => model.set_parent(bone, parent).map(Picked::Bone),
            (Picked::Bone(bone), None) => model.set_parent(bone, None).map(Picked::Bone),
            (Picked::Box(bone, index), Some(Some(to))) => {
                model.move_box(bone, index, to).map(|index| Picked::Box(to, index))
            }
            (Picked::Box(..), _) => None,
        };
        if let Some(made) = made {
            self.change(model, None);
            self.pick(Some(made));
        }
    }

    fn update_view(&mut self, ui: &mut Ui, renderer: &mut Renderer, scale: f32) {
        let Some(open) = self.open.as_ref() else { return };
        let mode = match self.mode {
            Mode::Shape => ViewMode::Shape,
            Mode::Paint => ViewMode::Paint { mirror: ui.is_checked(self.symmetry) },
        };
        let mut events = self.view.update(ui, renderer, &open.model, open.picked, mode, scale);
        if self.mode == Mode::Paint {
            events.extend(self.sheet.update(ui, &open.model, open.picked));
        }
        for event in events {
            match event {
                ViewEvent::Paint { pixel, within } => self.paint(pixel, within),
                ViewEvent::PaintEnded => {
                    if let Some(open) = self.open.as_mut() {
                        open.history.seal();
                    }
                }
                ViewEvent::Pick(picked) => self.pick(picked),
                ViewEvent::DragBegan(_) => {
                    if let Some(open) = self.open.as_mut() {
                        open.held = Some(open.model.clone());
                    }
                }
                ViewEvent::Dragged(handle, pixels) => self.drag(handle, pixels),
                ViewEvent::DragEnded => {
                    if let Some(open) = self.open.as_mut() {
                        open.held = None;
                        open.history.seal();
                    }
                }
            }
        }
    }

    /// A drag from a handle, `pixels` from where it began: what is picked
    /// moved or sized by that much from how the drag found it.
    fn drag(&mut self, handle: Handle, pixels: f32) {
        let Some((held, picked)) = self.open.as_ref().and_then(|open| Some((open.held.clone()?, open.picked?))) else {
            return;
        };
        let mut model = held;
        match (handle, picked) {
            (Handle::Move(axis), Picked::Bone(bone)) => {
                let mut pivot = model.bones[bone].pivot;
                pivot[axis] += pixels;
                match self.pivot_alone {
                    true => model.move_pivot(bone, pivot),
                    false => model.bones[bone].pivot = pivot,
                }
            }
            (Handle::Move(axis), Picked::Box(bone, index)) => model.bones[bone].boxes[index].from[axis] += pixels,
            (Handle::Face(face), Picked::Box(bone, index)) => {
                let model_box = &mut model.bones[bone].boxes[index];
                let axis = match face {
                    Face::Right | Face::Left => 0,
                    Face::Top | Face::Bottom => 1,
                    Face::Front | Face::Back => 2,
                };
                let was = model_box.size[axis] as f32;
                let now = (was + pixels).max(0.0);
                model_box.size[axis] = now as u32;
                // A face on the low side takes the corner the box is measured from with it.
                if matches!(face, Face::Left | Face::Bottom | Face::Back) {
                    model_box.from[axis] -= now - was;
                }
            }
            (Handle::Face(_), Picked::Bone(_)) => return,
        }
        self.change(model, Some(EditKey::Drag));
    }

    /// Shows the model as it now is in the tree and the view, once it or
    /// what is picked changed.
    fn show(&mut self, ui: &mut Ui, renderer: &mut Renderer) {
        let Some(open) = self.open.as_mut().filter(|open| open.stale || open.repainted) else { return };
        let (reshaped, repainted) = (open.stale, open.repainted);
        (open.stale, open.repainted) = (false, false);
        if repainted {
            self.sheet.sheet_changed(renderer, &open.model.sheet);
        }
        if !reshaped {
            return self.view.sheet_changed(renderer, &open.model);
        }
        let rows = tree_rows(&open.model);
        let boxes: usize = open.model.bones.iter().map(|bone| bone.boxes.len()).sum();
        let summary = format!("{} bones · {} boxes", open.model.bones.len(), boxes);
        if ui.text(self.summary) != summary {
            ui.set_text(self.summary, &summary);
        }
        if self.rows.borrow().0 != rows {
            let bones: Vec<u64> = rows.iter().filter(|row| row.icon == "branch").map(|row| row.id).collect();
            self.rows.borrow_mut().0 = rows;
            ui.refresh_list(self.tree);
            for bone in bones {
                ui.set_expanded(self.tree, bone, true);
            }
        }
        let lit: Vec<u64> = open.picked.map(tree_id).into_iter().collect();
        if ui.selected_items(self.tree) != lit {
            ui.select_items(self.tree, &lit);
        }
        self.view.model_changed(renderer, &open.model);
    }
}

/// Whether a model has a part.
fn has(model: &BoxModel, part: Picked) -> bool {
    let Some(bone) = model.bones.get(part.bone()) else { return false };
    match part {
        Picked::Bone(_) => true,
        Picked::Box(_, index) => index < bone.boxes.len(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tree_row_is_a_bone_or_one_of_its_boxes() {
        for part in [Picked::Bone(0), Picked::Bone(7), Picked::Box(0, 0), Picked::Box(7, 31)] {
            assert_eq!(tree_part(tree_id(part)), part);
        }
        let mut model = BoxModel::starter();
        let head = model.add_bone("Head", Some(0), Vec3::ZERO);
        model.add_box(head, Vec3::ZERO, [2, 2, 2], [0, 0]);
        let rows = tree_rows(&model);
        let shown: Vec<(&str, Option<u64>)> = rows.iter().map(|row| (row.name.as_str(), row.parent)).collect();
        assert_eq!(shown, [("Body", None), ("Box 1", Some(0)), ("Head", Some(0)), ("Box 1", Some(BONE))]);
    }

    #[test]
    fn changes_close_together_to_one_thing_are_one_step() {
        let mut history = History::default();
        let (first, second) = (BoxModel::starter(), BoxModel::default());
        history.record(&first, None, Some(EditKey::Drag));
        history.record(&second, None, Some(EditKey::Drag));
        assert_eq!(history.done.len(), 1, "one drag");
        history.seal();
        history.record(&second, None, Some(EditKey::Drag));
        assert_eq!(history.done.len(), 2, "another drag");
        history.record(&second, None, None);
        assert_eq!(history.done.len(), 3, "a change with nothing to merge with");
    }
}
