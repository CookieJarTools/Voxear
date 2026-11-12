//! The Model window's timeline: a box model's clips, and the keys of the
//! one picked, laid out along its length. Over it are the clip's own
//! things: which clip, its name, how long it is and whether it loops, Play,
//! and Key and Remove Key for the bone picked at the moment shown.
//!
//! Under them each bone the clip moves has a row, and the bone picked has
//! one whether it has keys yet or not, each key a dot at its moment. A
//! press on the timeline moves the moment shown there, and picks the bone
//! whose row it is in; a press on a key drags it to another moment. Keys
//! and the moment shown stand on steps (`KEY_STEP`).
//!
//! The view changes nothing itself: what was asked of it comes back as
//! [`TimelineEvent`]s for the window to make steps to undo of.

use voxear_render::{BoxModel, KEY_STEP, LONGEST_CLIP, on_step};
use voxear_ui::{Color, NodeId, Shape, Style, Ui};

use crate::inspector::{read_numbers, write_numbers};

/// How tall the strip of times over the rows is, and each row, in logical pixels.
const RULER: f32 = 18.0;
const ROW: f32 = 20.0;
/// How much room the timeline leaves at each end of the clip.
const PAD: f32 = 12.0;
/// How near a key a press takes hold of it.
const KEY_REACH: f32 = 7.0;
/// The least room between two times written over the timeline.
const TICK_ROOM: f32 = 46.0;

const LINE: Color = Color::rgba(255, 255, 255, 18);
const SECOND: Color = Color::rgba(255, 255, 255, 46);
const LIT_ROW: Color = Color::rgba(255, 196, 64, 10);
const KEY: Color = Color::rgb(176, 182, 194);
const LIT_KEY: Color = Color::rgb(255, 196, 64);
const MOMENT: Color = Color::rgb(86, 156, 255);
const RIM: Color = Color::rgb(20, 22, 26);

/// What the timeline asks of the window.
#[derive(Clone, Debug, PartialEq)]
pub enum TimelineEvent {
    NewClip,
    DeleteClip,
    /// Show this clip, by its place among the model's.
    PickClip(usize),
    Rename(String),
    /// Make the clip this many seconds long.
    Length(f32),
    Looping(bool),
    Play(bool),
    /// Key the bone picked at the moment shown, or take its key there away.
    AddKey,
    RemoveKey,
    /// Show this moment of the clip.
    Scrub(f32),
    PickBone(usize),
    /// A key of this bone dragged from the moment it was at when the drag
    /// began to another, and the drag let go.
    MoveKey {
        bone: usize,
        from: f32,
        to: f32,
    },
    MoveEnded,
}

/// What a press on the timeline has hold of.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Held {
    /// The moment shown.
    Moment,
    /// A key: where it was when the press took it, and where it now is.
    Key { bone: usize, from: f32, at: f32 },
}

pub struct TimelineView {
    frame: NodeId,
    clips: NodeId,
    clip_names: Vec<String>,
    new_clip: NodeId,
    delete_clip: NodeId,
    name: NodeId,
    length: NodeId,
    looping: NodeId,
    play: NodeId,
    moment: NodeId,
    add_key: NodeId,
    remove_key: NodeId,
    /// The bones' names down the left, one a row, and which they show.
    names: NodeId,
    rows: Vec<(usize, String, bool)>,
    /// The rows themselves, which take the presses, the times written over
    /// them, and what is drawn on them.
    lane: NodeId,
    ticks: NodeId,
    ticked: (f32, f32),
    overlay: NodeId,
    held: Option<Held>,
}

impl TimelineView {
    pub fn new(ui: &mut Ui, parent: NodeId) -> Self {
        let frame = ui.panel(parent, Style::column());
        ui.add_class(frame, "model-timeline");
        let bar = ui.panel(frame, Style::row());
        ui.add_class(bar, "model-timeline-bar");
        let button = |ui: &mut Ui, text: &str, tip: &str| {
            let button = ui.button(bar, text);
            ui.add_class(button, "console-button");
            ui.set_tooltip(button, tip, None);
            button
        };
        let clips = ui.dropdown(bar, &["No clips"], 0);
        ui.add_class(clips, "model-timeline-clips");
        let new_clip = button(ui, "New Clip", "A clip with no keys");
        let delete_clip = button(ui, "Delete Clip", "Takes the clip shown out of the model");
        let label = |ui: &mut Ui, text: &str| {
            let label = ui.add_label(bar, text);
            ui.add_class(label, "model-timeline-label");
        };
        label(ui, "Name");
        let name = ui.text_field(bar, "");
        ui.add_class(name, "value-input");
        ui.add_class(name, "model-timeline-name-field");
        label(ui, "Length");
        let length = ui.text_field(bar, "");
        ui.add_class(length, "value-input");
        ui.add_class(length, "number-input");
        ui.add_class(length, "model-timeline-length");
        let looping = button(ui, "Loop", "Goes on from its last key round to its first");
        let play = button(ui, "Play", "Plays the clip, or stops it where it is");
        let moment = ui.add_label(bar, "");
        ui.add_class(moment, "model-timeline-moment");
        let add_key = button(ui, "Key", "Keys the bone picked as it stands at the moment shown");
        let remove_key = button(ui, "Remove Key", "Takes away the key the bone picked has at the moment shown");

        let body = ui.panel(frame, Style::row());
        ui.add_class(body, "model-timeline-body");
        let names = ui.panel(body, Style::column());
        ui.add_class(names, "model-timeline-names");
        let lane = ui.panel(body, Style::default());
        ui.add_class(lane, "model-timeline-lane");
        let overlay = ui.panel(lane, Style { pickable: false, ..Style::default() });
        ui.add_class(overlay, "model-view-overlay");
        let ticks = ui.panel(lane, Style { pickable: false, ..Style::default() });
        ui.add_class(ticks, "model-view-overlay");
        Self {
            frame,
            clips,
            clip_names: Vec::new(),
            new_clip,
            delete_clip,
            name,
            length,
            looping,
            play,
            moment,
            add_key,
            remove_key,
            names,
            rows: Vec::new(),
            lane,
            ticks,
            ticked: (0.0, 0.0),
            overlay,
            held: None,
        }
    }

    /// The whole timeline, to show while clips are worked on.
    pub fn frame_node(&self) -> NodeId {
        self.frame
    }

    /// Whether a press has hold of a key, or of the moment shown.
    pub fn is_dragging(&self) -> bool {
        self.held.is_some()
    }

    /// The timeline's frame: its bar and rows shown as `model`'s clip
    /// `clip` now is, `time` seconds in, with `picked` the bone picked, and
    /// what was pressed, typed and dragged in it.
    pub fn update(
        &mut self,
        ui: &mut Ui,
        model: &BoxModel,
        clip: usize,
        time: f32,
        playing: bool,
        picked: Option<usize>,
    ) -> Vec<TimelineEvent> {
        let mut events = Vec::new();
        self.update_bar(ui, model, clip, time, playing, picked, &mut events);
        let shown = model.clips.get(clip);
        let length = shown.map_or(1.0, |clip| clip.length).max(KEY_STEP);

        // A row for each bone the clip moves, and for the one picked.
        let moved = |name: &str| shown.is_some_and(|clip| clip.track(name).is_some());
        let rows: Vec<(usize, String, bool)> = (model.bones.iter().enumerate())
            .filter(|(at, bone)| moved(&bone.name) || picked == Some(*at))
            .map(|(at, bone)| (at, bone.name.clone(), picked == Some(at)))
            .collect();
        if rows != self.rows {
            for child in ui.children(self.names) {
                ui.remove(child);
            }
            for (_, name, lit) in &rows {
                let label = ui.add_label(self.names, name);
                ui.add_class(label, "model-timeline-row");
                if *lit {
                    ui.add_class(label, "model-timeline-row-lit");
                }
            }
            self.rows = rows;
        }

        let [left, top, width, _] = ui.rect(self.lane);
        let span = (width - 2.0 * PAD).max(1.0);
        let x_of = |time: f32| PAD + time / length * span;
        let time_of = |x: f32| on_step(((x - PAD) / span * length).clamp(0.0, length));
        self.write_times(ui, length, span);

        let local = ui.pointer_position().map(|(x, y)| (x - left, y - top));
        let row_at = |y: f32| ((y - RULER) / ROW).floor();
        let rows = &self.rows;
        // The bone whose row a point is in, and its key within reach of the point.
        let under = |(x, y): (f32, f32)| {
            let row = row_at(y);
            let (bone, name, _) = rows.get(row as usize).filter(|_| row >= 0.0)?;
            let keys = shown.and_then(|clip| clip.track(name)).map(|track| track.keys.as_slice()).unwrap_or_default();
            let near = keys.iter().map(|key| (key.time, (x_of(key.time) - x).abs()));
            let key = near.filter(|(_, away)| *away <= KEY_REACH).min_by(|a, b| a.1.total_cmp(&b.1));
            Some((*bone, key.map(|(time, _)| time)))
        };
        let pressed = ui.is_pressed(self.lane);
        match (pressed, local, self.held) {
            // A press begins: on a key, or on the timeline.
            (true, Some(at), None) => match under(at) {
                Some((bone, Some(key))) => {
                    events.extend([TimelineEvent::PickBone(bone), TimelineEvent::Scrub(key)]);
                    self.held = Some(Held::Key { bone, from: key, at: key });
                }
                found => {
                    events.extend(found.map(|(bone, _)| TimelineEvent::PickBone(bone)));
                    events.push(TimelineEvent::Scrub(time_of(at.0)));
                    self.held = Some(Held::Moment);
                }
            },
            (true, Some((x, _)), Some(Held::Moment)) => {
                if (time_of(x) - time).abs() > KEY_STEP * 0.5 {
                    events.push(TimelineEvent::Scrub(time_of(x)));
                }
            }
            (true, Some((x, _)), Some(Held::Key { bone, from, at })) => {
                let to = time_of(x);
                if (to - at).abs() > KEY_STEP * 0.5 {
                    events.extend([TimelineEvent::MoveKey { bone, from, to }, TimelineEvent::Scrub(to)]);
                    self.held = Some(Held::Key { bone, from, at: to });
                }
            }
            (true, None, _) => {}
            (false, at, held) => {
                // A press made and let go between two frames shows its moment all the same.
                if let (None, true, Some(at)) = (held, ui.clicked(self.lane), at) {
                    events.extend(under(at).map(|(bone, _)| TimelineEvent::PickBone(bone)));
                    events.push(TimelineEvent::Scrub(time_of(at.0)));
                }
                if matches!(self.held.take(), Some(Held::Key { .. })) {
                    events.push(TimelineEvent::MoveEnded);
                }
            }
        }

        // The rows, the lines of the times, each key, and the moment shown over all.
        let mut shapes = Vec::new();
        let bottom = RULER + (self.rows.len().max(1) as f32) * ROW;
        for (row, (_, _, lit)) in self.rows.iter().enumerate() {
            let y = RULER + row as f32 * ROW;
            if *lit {
                shapes.push(Shape::Rect { rect: [0.0, y, width, ROW], radius: 0.0, fill: LIT_ROW });
            }
            shapes.push(Shape::Line { from: [0.0, y + ROW], to: [width, y + ROW], width: 1.0, color: LINE });
        }
        let step = tick_step(length, span);
        for tick in 0..=(length / step + 1e-3).floor() as u32 {
            let at = tick as f32 * step;
            let whole = (at - at.round()).abs() < 1e-3;
            let x = x_of(at);
            let color = if whole { SECOND } else { LINE };
            shapes.push(Shape::Line { from: [x, RULER - 4.0], to: [x, bottom], width: 1.0, color });
        }
        let end = x_of(length);
        shapes.push(Shape::Line { from: [end, 0.0], to: [end, bottom], width: 1.5, color: SECOND });
        for (row, (_, name, lit)) in self.rows.iter().enumerate() {
            let Some(track) = shown.and_then(|clip| clip.track(name)) else { continue };
            let y = RULER + (row as f32 + 0.5) * ROW;
            for key in &track.keys {
                let now = *lit && (key.time - time).abs() < KEY_STEP * 0.5;
                let fill = if now {
                    Color::WHITE
                } else if *lit {
                    LIT_KEY
                } else {
                    KEY
                };
                shapes.push(Shape::Circle { center: [x_of(key.time), y], radius: 4.5, fill, border: Some((1.0, RIM)) });
            }
        }
        let x = x_of(time.clamp(0.0, length));
        shapes.push(Shape::Line { from: [x, 0.0], to: [x, bottom], width: 1.5, color: MOMENT });
        shapes.push(Shape::Rect { rect: [x - 4.0, 0.0, 8.0, 4.0], radius: 1.5, fill: MOMENT });
        ui.set_shapes(self.overlay, shapes);
        events
    }

    /// The bar over the rows: shown as the clip is, and what its buttons
    /// and fields were asked.
    #[allow(clippy::too_many_arguments)]
    fn update_bar(
        &mut self,
        ui: &mut Ui,
        model: &BoxModel,
        clip: usize,
        time: f32,
        playing: bool,
        picked: Option<usize>,
        events: &mut Vec<TimelineEvent>,
    ) {
        let names: Vec<String> = model.clips.iter().map(|clip| clip.name.clone()).collect();
        if names != self.clip_names {
            let offered: Vec<&str> = names.iter().map(String::as_str).collect();
            ui.set_options(self.clips, if offered.is_empty() { &["No clips"] } else { &offered });
            self.clip_names = names;
            ui.set_selected_option(self.clips, clip.min(self.clip_names.len().saturating_sub(1)));
        }
        let chosen = ui.selected_option(self.clips);
        if ui.changed(self.clips) && chosen != clip && chosen < model.clips.len() {
            events.push(TimelineEvent::PickClip(chosen));
        } else if chosen != clip && clip < model.clips.len() {
            ui.set_selected_option(self.clips, clip);
        }
        if ui.clicked(self.new_clip) {
            events.push(TimelineEvent::NewClip);
        }
        let shown = model.clips.get(clip);
        let keyed = shown
            .zip(picked.and_then(|bone| model.bones.get(bone)))
            .map(|(clip, bone)| clip.key(&bone.name, time).is_some());
        for (button, enabled) in [
            (self.delete_clip, shown.is_some()),
            (self.looping, shown.is_some()),
            (self.play, shown.is_some()),
            (self.add_key, picked.is_some()),
            (self.remove_key, keyed == Some(true)),
        ] {
            if ui.is_enabled(button) != enabled {
                ui.set_enabled(button, enabled);
            }
        }
        if ui.clicked(self.add_key) && picked.is_some() {
            events.push(TimelineEvent::AddKey);
        }
        let moment = format!("{:.2} s", time);
        if ui.text(self.moment) != moment {
            ui.set_text(self.moment, &moment);
        }
        let Some(shown) = shown else {
            for field in [self.name, self.length] {
                if ui.focused() != Some(field) && !ui.text(field).is_empty() {
                    ui.set_text(field, "");
                }
            }
            return;
        };
        if ui.clicked(self.delete_clip) {
            events.push(TimelineEvent::DeleteClip);
        }
        if ui.clicked(self.remove_key) && keyed == Some(true) {
            events.push(TimelineEvent::RemoveKey);
        }
        if ui.clicked(self.looping) {
            events.push(TimelineEvent::Looping(!shown.looping));
        }
        if ui.clicked(self.play) {
            events.push(TimelineEvent::Play(!playing));
        }
        for (toggle, on) in [(self.looping, shown.looping), (self.play, playing)] {
            if ui.is_checked(toggle) != on {
                ui.set_checked(toggle, on);
            }
        }
        let typed = ui.text(self.name).trim().to_string();
        if ui.submitted(self.name) && !typed.is_empty() && typed != shown.name {
            events.push(TimelineEvent::Rename(typed));
        } else if ui.focused() != Some(self.name) && ui.text(self.name) != shown.name {
            ui.set_text(self.name, &shown.name);
        }
        let read = read_numbers(ui, &[self.length], &[shown.length])[0];
        if read != shown.length && read.is_finite() {
            events.push(TimelineEvent::Length(read.clamp(KEY_STEP, LONGEST_CLIP)));
        } else {
            write_numbers(ui, &[self.length], &[shown.length]);
        }
    }

    /// Writes the times over the rows, as far apart as leaves each room.
    fn write_times(&mut self, ui: &mut Ui, length: f32, span: f32) {
        if self.ticked == (length, span) {
            return;
        }
        self.ticked = (length, span);
        for child in ui.children(self.ticks) {
            ui.remove(child);
        }
        let step = tick_step(length, span);
        for tick in 0..=(length / step + 1e-3).floor() as u32 {
            let at = tick as f32 * step;
            let text = format!("{at:.2}");
            let label = ui.add_label(self.ticks, text.trim_end_matches('0').trim_end_matches('.'));
            ui.add_class(label, "model-timeline-tick");
            ui.set_inline_style(label, &format!("left: {}px", (PAD + at / length * span + 3.0).round()));
        }
    }

    /// Where a moment of a bone's row is drawn in the window, for a test
    /// to press on; `None` for a bone with no row.
    pub fn point(&self, ui: &Ui, model: &BoxModel, clip: usize, bone: usize, time: f32) -> Option<(f32, f32)> {
        let row = self.rows.iter().position(|(at, ..)| *at == bone)?;
        let [left, top, width, _] = ui.rect(self.lane);
        let length = model.clips.get(clip).map_or(1.0, |clip| clip.length).max(KEY_STEP);
        let x = PAD + time / length * (width - 2.0 * PAD).max(1.0);
        Some((left + x, top + RULER + (row as f32 + 0.5) * ROW))
    }

    /// The bar's buttons and fields, for a test to press and type in.
    pub fn new_clip_button(&self) -> NodeId {
        self.new_clip
    }

    pub fn key_button(&self) -> NodeId {
        self.add_key
    }

    pub fn remove_key_button(&self) -> NodeId {
        self.remove_key
    }

    pub fn length_field(&self) -> NodeId {
        self.length
    }

    /// The bones that have rows, from the top.
    pub fn rows(&self) -> Vec<usize> {
        self.rows.iter().map(|(bone, ..)| *bone).collect()
    }
}

/// How far apart the times written over a clip `length` seconds long are,
/// where it is drawn `span` logical pixels wide.
fn tick_step(length: f32, span: f32) -> f32 {
    let steps = [0.05, 0.1, 0.25, 0.5, 1.0, 2.0, 5.0, 10.0, 30.0];
    steps.into_iter().find(|step| step / length * span >= TICK_ROOM).unwrap_or(60.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn times_are_written_as_far_apart_as_leaves_each_room() {
        assert_eq!(tick_step(1.0, 600.0), 0.1);
        assert_eq!(tick_step(1.0, 200.0), 0.25);
        assert_eq!(tick_step(10.0, 600.0), 1.0);
        assert_eq!(tick_step(60.0, 300.0), 10.0);
        assert_eq!(tick_step(60.0, 20.0), 60.0);
    }
}
