//! The Gun Editor's panel: a Gun Handling worked on with a character
//! holding a gun in a view of its own. The top bar picks the handling, the
//! character and how the view looks: through the character's eyes, from
//! the side, from the front, or turned about by dragging. The bar under it
//! picks what the character does: holds the gun at the hip or aimed, fires
//! it, or plays one of its actions from an ammo state, at a speed, once or
//! round again, standing or walking on the spot. The timeline under the
//! view shows the action's beats, and the details beside it what is picked.
//! `editor::gun_handlings` makes the Objs the view shows and saves the
//! handling.

use std::sync::Arc;

use voxear_assets::AssetGuid;
use voxear_core::math::{Mat4, Quat, Vec3};
use voxear_ecs::Entity;
use voxear_engine::guns::{
    actions::{ActionId, ActionKind},
    handling::GunHandling,
    preview::PreviewStart,
};
use voxear_ui::{NodeId, Style, Ui};

use crate::{
    gun_details::{Details, Target},
    gun_timeline::Timeline,
    hand_handles::HandHandles,
};

const MARKUP: &str = include_str!("../ui/GunEditor.vui");

/// The speed menu's choices, in order.
pub(crate) const SPEEDS: [f32; 4] = [1.0, 0.5, 0.25, 0.1];
/// Edits kept to undo, at most.
const UNDO_DEPTH: usize = 100;
/// How far a drag turns the Orbit view, in radians per pixel, and how much
/// a wheel's notch brings it nearer.
const TURN_PER_PIXEL: f32 = 0.008;
const ZOOM_PER_NOTCH: f32 = 0.9;

/// How the view looks at the character.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum View {
    /// Through the character's own eyes, as its player sees.
    #[default]
    FirstPerson,
    Side,
    Front,
    /// From wherever drags have turned it.
    Orbit,
}

impl View {
    const ALL: [View; 4] = [View::FirstPerson, View::Side, View::Front, View::Orbit];

    pub fn index(self) -> usize {
        Self::ALL.iter().position(|&view| view == self).unwrap_or(0)
    }

    pub fn from_index(index: usize) -> Self {
        Self::ALL.get(index).copied().unwrap_or_default()
    }
}

/// The Orbit view's turn about the gun and its distance from it, and where
/// the pointer was last frame while a drag turns it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Orbit {
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
    dragging: Option<(f32, f32)>,
}

impl Default for Orbit {
    fn default() -> Self {
        Self { yaw: 2.4, pitch: 0.25, distance: 1.1, dragging: None }
    }
}

impl Orbit {
    /// Turns it by a drag over `viewport`, and brings it nearer or further
    /// by the wheel.
    pub fn steer(&mut self, ui: &Ui, viewport: NodeId) {
        let pointer = ui.pointer_position();
        match (ui.is_pressed(viewport), pointer, self.dragging) {
            (true, Some(now), Some(was)) => {
                self.yaw += (now.0 - was.0) * TURN_PER_PIXEL;
                self.pitch = (self.pitch + (now.1 - was.1) * TURN_PER_PIXEL).clamp(-1.4, 1.4);
                self.dragging = Some(now);
            }
            (true, Some(now), None) => self.dragging = Some(now),
            _ => self.dragging = None,
        }
        let notches = ui.wheeled(viewport)[1];
        if notches != 0.0 {
            self.distance = (self.distance * ZOOM_PER_NOTCH.powf(notches)).clamp(0.25, 6.0);
        }
    }

    /// Where its camera is and which way it looks, about `target` for a
    /// character facing `facing`.
    pub fn camera(&self, target: Vec3, facing: Quat) -> (Vec3, Quat) {
        let way = facing * Quat::from_rotation_y(self.yaw) * Quat::from_rotation_x(-self.pitch) * Vec3::Z;
        let at = target - way * self.distance;
        (at, look_along(way))
    }
}

/// The turn that looks along `way`, upright.
pub(crate) fn look_along(way: Vec3) -> Quat {
    let way = way.normalize_or_zero();
    let yaw = way.x.atan2(way.z);
    let pitch = way.y.clamp(-1.0, 1.0).asin();
    Quat::from_rotation_y(yaw) * Quat::from_rotation_x(-pitch)
}

/// Where a fixed view's camera is, and which way it looks, about where a
/// gun is held, `gun`, by a character facing `facing`: from its right side, or
/// from in front of it.
pub(crate) fn fixed_camera(view: View, gun: Mat4, facing: Quat) -> (Vec3, Quat) {
    let target = gun.w_axis.truncate();
    let (way, distance) = match view {
        View::Front => (facing * Vec3::NEG_Z, 1.2),
        _ => (facing * Vec3::NEG_X, 0.95),
    };
    (target - way * distance, look_along(way))
}

/// The Gun Handling open in the editor: its file, as written now, and the
/// steps to take back and do again. While held, as a drag or typing in a
/// field holds it, its changes are one step.
pub(crate) struct Document {
    pub path: String,
    pub guid: AssetGuid,
    /// How a Gun refers to it, as its Handling field writes.
    pub reference: String,
    pub handling: GunHandling,
    /// The same, to give the guns picking it.
    pub shared: Arc<GunHandling>,
    /// Whether it has changed since it was written.
    pub unsaved: bool,
    undo: Vec<GunHandling>,
    redo: Vec<GunHandling>,
    /// Whether it is held, and whether the step before the hold is kept.
    held: bool,
    kept: bool,
}

impl Document {
    pub fn new(path: String, guid: AssetGuid, reference: String, handling: GunHandling) -> Self {
        let shared = Arc::new(handling.clone());
        let (undo, redo) = (Vec::new(), Vec::new());
        Self { path, guid, reference, handling, shared, unsaved: false, undo, redo, held: false, kept: false }
    }

    /// Changes it: a step of its own, or part of the hold's.
    pub fn edit(&mut self, change: impl FnOnce(&mut GunHandling)) {
        let mut changed = self.handling.clone();
        change(&mut changed);
        if changed == self.handling {
            return;
        }
        if !(self.held && self.kept) {
            self.keep_step();
            self.kept = self.held;
        }
        self.handling = changed;
        self.shared = Arc::new(self.handling.clone());
        self.unsaved = true;
    }

    /// Holds it, or lets it go: a hold's changes are one step, written once
    /// it is let go.
    pub fn hold(&mut self, held: bool) {
        if !held {
            self.kept = false;
        }
        self.held = held;
    }

    /// Whether it should be written now: changed, and not held.
    pub fn ready_to_write(&self) -> bool {
        self.unsaved && !self.held
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Takes back the last step; false with none to take back.
    pub fn undo(&mut self) -> bool {
        let Some(before) = self.undo.pop() else { return false };
        self.redo.push(std::mem::replace(&mut self.handling, before));
        self.changed_by_history();
        true
    }

    /// Does again the last step taken back; false with none.
    pub fn redo(&mut self) -> bool {
        let Some(after) = self.redo.pop() else { return false };
        self.undo.push(std::mem::replace(&mut self.handling, after));
        self.changed_by_history();
        true
    }

    fn keep_step(&mut self) {
        self.undo.push(self.handling.clone());
        if self.undo.len() > UNDO_DEPTH {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    fn changed_by_history(&mut self) {
        (self.held, self.kept) = (false, false);
        self.shared = Arc::new(self.handling.clone());
        self.unsaved = true;
    }
}

/// The Objs the view shows: the character's instance and the holder in
/// it, the gun it holds, and the camera the view looks through. Made for
/// the editor alone, far from the level.
pub(crate) struct Preview {
    /// The character's prefab, by its path, and the Gun it holds in place
    /// of its own, if the editor was opened from one.
    pub character: String,
    pub source: Option<Entity>,
    /// The handling its gun was given, by its path.
    pub handling: String,
    pub root: Entity,
    pub holder: Entity,
    pub gun: Entity,
    pub camera: Entity,
    /// What hides the character's head from its own eyes, shown in the
    /// views from outside.
    pub head_hiders: Vec<Entity>,
    /// Where it stands, which way it faces.
    pub spot: Vec3,
    pub facing: Quat,
    /// What the views from outside look at: where a gun is held before the
    /// character's eyes, once it has stood a moment. They do not follow the
    /// gun, so a change to where it is held shows as a move.
    pub focus: Option<Vec3>,
    /// How many frames it has been shown.
    pub frames: u32,
}

/// The Gun Editor's panel.
pub(crate) struct GunEditor {
    /// Its dock panel's content.
    pub panel: NodeId,
    pub handling: NodeId,
    handling_name: NodeId,
    pub new: NodeId,
    pub character: NodeId,
    pub view: NodeId,
    pub undo: NodeId,
    pub redo: NodeId,
    pub pose: NodeId,
    pub fire: NodeId,
    actions: NodeId,
    pub play: NodeId,
    play_icon: NodeId,
    pub looping: NodeId,
    pub speed: NodeId,
    pub ammo: NodeId,
    pub walk: NodeId,
    pub viewport: NodeId,
    pub handles: NodeId,
    empty: NodeId,
    time_chip: NodeId,
    pub details: Details,
    pub timeline: Timeline,
    pub problems: NodeId,
    pub add_action: NodeId,
    /// The beat picked on the timeline, whose details show.
    pub picked: Option<Target>,
    /// Where the beat dragged along started, while it is.
    pub shift_from: Option<f32>,
    /// The view's handles on the hands and the gun.
    pub hands: HandHandles,
    pub doc: Option<Document>,
    /// The prefabs with a Gun Holder the character menu offers, by path,
    /// and the frame they were last looked for.
    pub characters: Vec<String>,
    pub characters_looked: Option<u64>,
    /// The action buttons, with what each plays.
    action_buttons: Vec<(NodeId, ActionId)>,
    /// The action showing, how far into it, in seconds, and whether time
    /// runs on, round again when Loop is on.
    pub showing: Option<ActionId>,
    pub time: f32,
    pub running: bool,
    pub looped: bool,
    pub orbit: Orbit,
    pub preview: Option<Preview>,
    /// The character that could not be made, not tried again until picked again.
    pub failed: Option<String>,
    /// The Gun the editor was opened from, which the character holds.
    pub source: Option<Entity>,
    /// What the problems strip shows, to make it again when that changes,
    /// and its button taking away offsets that do nothing.
    problems_shown: Option<Vec<String>>,
    remove_stale: Option<NodeId>,
}

impl GunEditor {
    /// Builds it in its dock panel's `content`.
    pub fn new(ui: &mut Ui, content: NodeId) -> Self {
        let mut load = |path: &str| Err(format!("{path} is not built in"));
        let root =
            ui.load_markup(content, "GunEditor.vui", MARKUP, &mut load).expect("the Gun Editor's markup is built in");
        let find = |ui: &Ui, name: &str| ui.find(root, name).unwrap_or_else(|| panic!("GunEditor.vui has no {name}"));
        let editor = Self {
            panel: content,
            handling: find(ui, "gun-handling"),
            handling_name: find(ui, "gun-handling-name"),
            new: find(ui, "gun-new"),
            character: find(ui, "gun-character"),
            view: find(ui, "gun-view"),
            undo: find(ui, "gun-undo"),
            redo: find(ui, "gun-redo"),
            pose: find(ui, "gun-pose"),
            fire: find(ui, "gun-fire"),
            actions: find(ui, "gun-actions"),
            play: find(ui, "gun-play"),
            play_icon: find(ui, "gun-play-icon"),
            looping: find(ui, "gun-loop"),
            speed: find(ui, "gun-speed"),
            ammo: find(ui, "gun-ammo"),
            walk: find(ui, "gun-walk"),
            viewport: find(ui, "gun-viewport"),
            handles: find(ui, "gun-handles"),
            empty: find(ui, "gun-empty"),
            time_chip: find(ui, "gun-time"),
            details: Details::new(find(ui, "gun-details")),
            timeline: Timeline::new(find(ui, "gun-timeline")),
            problems: find(ui, "gun-problems"),
            add_action: find(ui, "gun-add-action"),
            picked: None,
            shift_from: None,
            hands: HandHandles::default(),
            doc: None,
            characters: Vec::new(),
            characters_looked: None,
            action_buttons: Vec::new(),
            showing: None,
            time: 0.0,
            running: false,
            looped: false,
            orbit: Orbit::default(),
            preview: None,
            failed: None,
            source: None,
            problems_shown: None,
            remove_stale: None,
        };
        ui.set_tooltip(editor.new, "New Gun Handling", None);
        ui.set_tooltip(editor.undo, "Undo", Some("Ctrl+Z"));
        ui.set_tooltip(editor.redo, "Redo", Some("Ctrl+Y"));
        ui.set_tooltip(editor.looping, "Loop", None);
        ui.set_tooltip(editor.add_action, "New Action", None);
        ui.set_tooltip(editor.play, "Play", None);
        editor
    }

    pub fn view_shown(&self, ui: &Ui) -> View {
        View::from_index(ui.segment(self.view))
    }

    /// The ammo state actions start from: Full, Half, Empty or Dry. A gun
    /// as it is would change under an action shown again every frame.
    pub fn start(&self, ui: &Ui) -> PreviewStart {
        PreviewStart::ALL.get(ui.segment(self.ammo) + 1).copied().unwrap_or(PreviewStart::Full)
    }

    pub fn speed_picked(&self, ui: &Ui) -> f32 {
        SPEEDS.get(ui.selected_option(self.speed)).copied().unwrap_or(1.0)
    }

    pub fn aiming(&self, ui: &Ui) -> bool {
        ui.segment(self.pose) == 1
    }

    pub fn walking(&self, ui: &Ui) -> bool {
        ui.is_checked(self.walk)
    }

    /// The action button clicked this frame, if one was.
    pub fn action_clicked(&self, ui: &Ui) -> Option<ActionId> {
        self.action_buttons.iter().find(|(button, _)| ui.clicked(*button)).map(|(_, id)| id.clone())
    }

    /// Makes a button for each action the handling has for a gun of
    /// `kind`, if they changed: the built-in ones, then its own, each
    /// greyed when it does nothing for the gun.
    pub fn show_actions(&mut self, ui: &mut Ui, actions: &[(ActionId, bool)]) {
        let same = self.action_buttons.len() == actions.len()
            && self.action_buttons.iter().zip(actions).all(|((_, a), (b, _))| a == b);
        if !same {
            for (button, _) in self.action_buttons.drain(..) {
                ui.remove(button);
            }
            for (id, _) in actions {
                let button = ui.button(self.actions, &action_title(id));
                ui.add_class(button, "tool-toggle");
                ui.add_class(button, "gun-action");
                self.action_buttons.push((button, id.clone()));
            }
        }
        for ((button, id), (_, works)) in self.action_buttons.iter().zip(actions) {
            ui.set_enabled(*button, *works);
            ui.set_checked(*button, self.showing.as_ref() == Some(id));
        }
    }

    /// Shows the handling's name, whether time runs, Loop, and how far
    /// into its action the view is; and the note over the view when it
    /// shows nothing, saying why.
    pub fn show_state(&self, ui: &mut Ui, empty: Option<&str>, length: Option<f32>) {
        let name = self.doc.as_ref().map_or("None".to_string(), |doc| {
            let file = doc.path.rsplit('/').next().unwrap_or(&doc.path);
            let file = file.strip_suffix(".gunhandling").unwrap_or(file);
            if doc.unsaved { format!("{file} *") } else { file.to_string() }
        });
        if ui.text(self.handling_name) != name {
            ui.set_text(self.handling_name, &name);
        }
        ui.set_icon(self.play_icon, if self.running { "pause" } else { "play" });
        ui.set_checked(self.looping, self.looped);
        let can = |doc: &Document, undo: bool| if undo { doc.can_undo() } else { doc.can_redo() };
        ui.set_enabled(self.undo, self.doc.as_ref().is_some_and(|doc| can(doc, true)));
        ui.set_enabled(self.redo, self.doc.as_ref().is_some_and(|doc| can(doc, false)));
        ui.set_visible(self.empty, empty.is_some());
        if let Some(why) = empty
            && ui.text(self.empty) != why
        {
            ui.set_text(self.empty, why);
        }
        let time = match (&self.showing, length) {
            (Some(_), Some(length)) => format!("{:.2} / {:.2} s", self.time.min(length), length),
            _ => String::new(),
        };
        ui.set_visible(self.time_chip, !time.is_empty() && empty.is_none());
        if ui.text(self.time_chip) != time {
            ui.set_text(self.time_chip, &time);
        }
    }

    /// Fills the character menu with `characters`, picking `picked`.
    pub fn show_characters(&mut self, ui: &mut Ui, characters: Vec<String>, picked: Option<&str>) {
        if characters != self.characters {
            let mut options = vec!["No character".to_string()];
            options.extend(characters.iter().map(|path| prefab_name(path)));
            let labels: Vec<&str> = options.iter().map(String::as_str).collect();
            ui.set_options(self.character, &labels);
            self.characters = characters;
        }
        let at = picked.and_then(|path| self.characters.iter().position(|c| c == path)).map_or(0, |at| at + 1);
        if ui.selected_option(self.character) != at {
            ui.set_selected_option(self.character, at);
        }
    }

    /// The character the menu picks, by its prefab's path.
    pub fn character_picked(&self, ui: &Ui) -> Option<&str> {
        let at = ui.selected_option(self.character).checked_sub(1)?;
        self.characters.get(at).map(String::as_str)
    }

    /// Starts `id` from its start, playing, with no beat picked.
    pub fn play_action(&mut self, id: ActionId) {
        self.picked = None;
        self.showing = Some(id);
        self.time = 0.0;
        self.running = true;
    }

    /// Moves time on by `delta` seconds of the editor's, at the speed
    /// picked, for an action `length` seconds long: round again with Loop
    /// on, after a moment at its end, or else stopping there.
    pub fn advance(&mut self, delta: f32, speed: f32, length: f32) {
        if !self.running || self.showing.is_none() {
            return;
        }
        self.time += delta * speed;
        // A moment at the end, so the last beat is seen to finish.
        let end = length + 0.35;
        if self.time >= end {
            if self.looped {
                self.time = 0.0;
            } else {
                (self.time, self.running) = (length, false);
            }
        }
    }

    /// Lists what is wrong in the strip under the timeline, a warning sign
    /// on each, with a button that takes away offsets that do nothing when
    /// there are any.
    pub fn show_problems(&mut self, ui: &mut Ui, problems: &[String], stale: bool) {
        let mut shown = problems.to_vec();
        shown.push(stale.to_string());
        if self.problems_shown.as_ref() == Some(&shown) {
            return;
        }
        self.problems_shown = Some(shown);
        for row in ui.children(self.problems) {
            ui.remove(row);
        }
        self.remove_stale = None;
        if problems.is_empty() {
            let fine = ui.add_label(self.problems, "No problems");
            ui.add_class(fine, "gun-fine");
            return;
        }
        for problem in problems {
            let row = ui.panel(self.problems, Style::row());
            ui.add_class(row, "gun-problem");
            let sign = ui.icon(row, "alert");
            ui.add_class(sign, "row-icon");
            ui.add_label(row, problem);
        }
        if stale {
            let button = ui.button(self.problems, "Remove Unused Offsets");
            ui.add_class(button, "header-button");
            self.remove_stale = Some(button);
        }
    }

    /// Whether the button taking away offsets that do nothing was clicked.
    pub fn remove_stale_clicked(&self, ui: &Ui) -> bool {
        self.remove_stale.is_some_and(|button| ui.clicked(button))
    }
}

/// An action's name, as its button shows it.
pub(crate) fn action_title(id: &ActionId) -> String {
    match id {
        ActionId::Builtin(kind) => kind.title().to_string(),
        ActionId::Own(name) => name.to_string(),
    }
}

/// The actions a gun with `handling` has: the built-in ones, then those of
/// its own that take no built-in's place.
pub(crate) fn actions_of(handling: &GunHandling) -> Vec<ActionId> {
    let builtin = ActionKind::ALL.into_iter().map(ActionId::Builtin);
    let own =
        handling.actions.iter().filter(|own| own.replaces.is_none()).map(|own| ActionId::Own(own.name.as_str().into()));
    builtin.chain(own).collect()
}

/// A prefab's name, from its path.
fn prefab_name(path: &str) -> String {
    let file = path.rsplit('/').next().unwrap_or(path);
    file.strip_suffix(".prefab").unwrap_or(file).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_drag_is_one_step_and_undo_takes_it_back_whole() {
        let guid = AssetGuid::new_random();
        let mut doc = Document::new("Rifle.gunhandling".into(), guid, String::new(), GunHandling::default());
        // Held with nothing changed is no step.
        doc.hold(true);
        doc.hold(false);
        assert!(!doc.can_undo());
        doc.hold(true);
        for step in 1..=5 {
            doc.edit(|handling| handling.poses.cant = step as f32);
        }
        assert!(!doc.ready_to_write(), "not written while dragging");
        doc.hold(false);
        assert!(doc.ready_to_write());
        doc.edit(|handling| handling.poses.return_pace = 2.0);
        assert!(doc.undo());
        assert_eq!((doc.handling.poses.cant, doc.handling.poses.return_pace), (5.0, 1.0));
        assert!(doc.undo());
        assert_eq!(doc.handling.poses.cant, 0.0, "the whole drag went at once");
        assert!(!doc.undo());
        assert!(doc.redo());
        assert_eq!(doc.handling.poses.cant, 5.0);
        // A change that changes nothing is not a step.
        doc.edit(|handling| handling.poses.cant = 5.0);
        assert!(doc.redo(), "the step after is still there to do again");
    }

    #[test]
    fn a_looking_turn_looks_where_it_was_asked_and_stays_upright() {
        for way in [Vec3::Z, Vec3::X, Vec3::new(-1.0, -0.5, 0.3), Vec3::new(0.2, 0.9, -1.0)] {
            let turn = look_along(way);
            assert!((turn * Vec3::Z - way.normalize()).length() < 1e-5, "{way}");
            assert!((turn * Vec3::X).y.abs() < 1e-5, "level across");
        }
    }
}
