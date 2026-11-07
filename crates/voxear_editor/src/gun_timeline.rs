//! The Gun Editor's timeline: the action showing, beat by beat, in a lane
//! for each hand, with the parts each beat moves or carries and the events
//! it ends with in lanes under them. A beat is a block, its reach fainter
//! than its work, named when there is room; beats that repeat together are
//! bracketed with how many times they play, and a beat with edits has a
//! dot. Pressing the ruler shows that time, and dragging along it scrubs.
//! A click picks a beat; dragging its right edge makes it longer or
//! shorter, and dragging it along starts it later or sooner.

use voxear_engine::guns::{
    actions::{Beat, BeatMark, Event, Hand},
    mechanism::PartRole,
};
use voxear_ui::{Color, NodeId, Shape, Ui};

/// The lanes' titles, down the left.
const LANES: [&str; 4] = ["Right Hand", "Left Hand", "Parts", "Events"];
const GUTTER: f32 = 84.0;
const RULER: f32 = 20.0;
const BRACKETS: f32 = 12.0;
const LANE: f32 = 24.0;
/// How near a block's right edge, in pixels, a press takes the edge.
const EDGE: f32 = 5.0;
/// How far, in pixels, a press must move before it drags a block along.
const SLOP: f32 = 3.0;
/// Time shown after the action ends, in seconds.
const TAIL: f32 = 0.35;

const LINE: u32 = 0x3a3c44;
const RIGHT: u32 = 0x6f9bff;
const LEFT: u32 = 0x58c49a;
const PART: u32 = 0xc9a25a;
const EVENT: u32 = 0xe8eaf0;
const PICKED: u32 = 0xffb44a;
const PLAYHEAD: u32 = 0xff6b5b;

/// What a drag on the timeline asks for.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum TimelineEdit {
    /// Shows this time, in seconds.
    Seek(f32),
    /// Picks a beat, by its place among those played, or none.
    Pick(Option<usize>),
    /// Makes the beat at this place this many seconds long.
    Retime { at: usize, length: f32 },
    /// Starts the beat at this place this many seconds later than it did
    /// when the drag began.
    Shift { at: usize, by: f32 },
    /// A drag that changed a beat is over.
    Done,
}

/// What is held: the playhead, or a beat's edge or the beat itself, from
/// where the press began and how long or where the beat was then.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Held {
    Playhead,
    Edge { at: usize, from: f32, length: f32 },
    Beat { at: usize, from: f32, moved: bool },
}

pub(crate) struct Timeline {
    pub canvas: NodeId,
    names: Vec<NodeId>,
    held: Option<Held>,
    /// How many seconds the width shows, kept while a drag is under way so
    /// the blocks do not move under the pointer.
    span: f32,
}

impl Timeline {
    pub fn new(canvas: NodeId) -> Self {
        Self { canvas, names: Vec::new(), held: None, span: 1.0 }
    }

    fn x(&self, width: f32, t: f32) -> f32 {
        GUTTER + t / self.span.max(0.1) * (width - GUTTER - 8.0).max(1.0)
    }

    fn t(&self, width: f32, x: f32) -> f32 {
        (x - GUTTER) / (width - GUTTER - 8.0).max(1.0) * self.span.max(0.1)
    }

    /// Draws the beats played, `time` seconds in, `picked` lit and those
    /// `edited` dotted.
    pub fn draw(&mut self, ui: &mut Ui, beats: &[(Beat, BeatMark)], time: f32, picked: Option<usize>, edited: &[bool]) {
        let [_, _, width, height] = ui.rect(self.canvas);
        let length: f32 = beats.iter().map(|(beat, _)| beat.length()).sum();
        if self.held.is_none() {
            self.span = length.max(0.5) + TAIL;
        }
        let mut shapes = Vec::new();
        let mut names: Vec<(String, f32, f32, bool)> = Vec::new();
        let lane_top = |lane: usize| RULER + BRACKETS + lane as f32 * LANE;
        // The ruler: a tick each tenth of a second, longer and numbered each second.
        for tenth in 0..=(self.span * 10.0) as usize {
            let x = self.x(width, tenth as f32 / 10.0);
            let whole = tenth % 10 == 0;
            let top = if whole {
                4.0
            } else if tenth % 5 == 0 {
                10.0
            } else {
                14.0
            };
            shapes.push(Shape::Line { from: [x, top], to: [x, RULER], width: 1.0, color: Color::hex(LINE) });
            if whole {
                names.push((format!("{} s", tenth / 10), x + 3.0, 2.0, true));
            }
        }
        for (lane, title) in LANES.iter().enumerate() {
            let top = lane_top(lane);
            shapes.push(Shape::Line { from: [0.0, top], to: [width, top], width: 1.0, color: Color::hex(LINE) });
            names.push((title.to_string(), 8.0, top + 5.0, true));
        }
        let mut start = 0.0;
        for (at, (beat, mark)) in beats.iter().enumerate() {
            let (left, middle, right) =
                (self.x(width, start), self.x(width, start + beat.reach), self.x(width, start + beat.length()));
            let lane = if beat.hand == Hand::Right { 0 } else { 1 };
            let top = lane_top(lane) + 3.0;
            let color = if picked == Some(at) {
                PICKED
            } else if lane == 0 {
                RIGHT
            } else {
                LEFT
            };
            let faint = Color::hex(color).with_alpha(90);
            shapes.push(Shape::Rect {
                rect: [left, top, (middle - left).max(1.0), LANE - 6.0],
                radius: 3.0,
                fill: faint,
            });
            let solid = Color::hex(color).with_alpha(230);
            shapes.push(Shape::Rect {
                rect: [middle, top, (right - middle).max(1.0), LANE - 6.0],
                radius: 3.0,
                fill: solid,
            });
            if edited.get(at).copied().unwrap_or(false) {
                shapes.push(Shape::Circle {
                    center: [right - 5.0, top + 4.0],
                    radius: 2.5,
                    fill: Color::hex(EVENT),
                    border: None,
                });
            }
            if right - left > 34.0 {
                names.push((beat.key.title().to_string(), left + 4.0, top + 2.0, false));
            }
            self.draw_parts(&mut shapes, &mut names, beat, (middle, right), lane_top(2) + 3.0);
            if beat.then != Event::None {
                let center = [right, lane_top(3) + LANE / 2.0];
                shapes.push(Shape::Circle { center, radius: 3.5, fill: Color::hex(EVENT), border: None });
                names.push((event_title(beat.then), right + 6.0, lane_top(3) + 5.0, true));
            }
            // A run that repeats: a bracket over its first time round, with how many times it plays.
            let first_of_run = mark.round == 0 && (at == 0 || beats[at - 1].1.run != mark.run);
            if mark.rounds > 1 && first_of_run {
                let first_round = beats[at..].iter().take_while(|(_, m)| m.run == mark.run && m.round == 0);
                let run: f32 = first_round.map(|(b, _)| b.length()).sum();
                let end = self.x(width, start + run);
                let y = RULER + BRACKETS - 4.0;
                shapes.push(Shape::Line { from: [left, y], to: [end, y], width: 1.5, color: Color::hex(PART) });
                shapes.push(Shape::Line { from: [left, y], to: [left, y + 4.0], width: 1.5, color: Color::hex(PART) });
                shapes.push(Shape::Line { from: [end, y], to: [end, y + 4.0], width: 1.5, color: Color::hex(PART) });
                names.push((format!("×{}", mark.rounds), left + 4.0, RULER - 1.0, true));
            }
            start += beat.length();
        }
        let x = self.x(width, time.min(self.span));
        shapes.push(Shape::Line { from: [x, 0.0], to: [x, height], width: 1.5, color: Color::hex(PLAYHEAD) });
        shapes.push(Shape::Circle { center: [x, 6.0], radius: 4.0, fill: Color::hex(PLAYHEAD), border: None });
        ui.set_shapes(self.canvas, shapes);
        self.show_names(ui, &names);
    }

    /// What the beat moves and carries, in the Parts lane, over its work.
    fn draw_parts(
        &self,
        shapes: &mut Vec<Shape>,
        names: &mut Vec<(String, f32, f32, bool)>,
        beat: &Beat,
        (from, to): (f32, f32),
        top: f32,
    ) {
        let part = beat.drive.map(|drive| drive.part).or(beat.carry);
        let Some(part) = part else { return };
        let fill = Color::hex(PART).with_alpha(170);
        shapes.push(Shape::Rect { rect: [from, top, (to - from).max(1.0), LANE - 6.0], radius: 3.0, fill });
        if to - from > 40.0 {
            names.push((part_title(part).to_string(), from + 4.0, top + 2.0, false));
        }
    }

    fn show_names(&mut self, ui: &mut Ui, names: &[(String, f32, f32, bool)]) {
        while self.names.len() < names.len() {
            let label = ui.add_label(self.canvas, "");
            ui.add_class(label, "gun-timeline-name");
            self.names.push(label);
        }
        for (index, &label) in self.names.iter().enumerate() {
            let Some((text, left, top, muted)) = names.get(index) else {
                ui.set_visible(label, false);
                continue;
            };
            if ui.text(label) != *text {
                ui.set_text(label, text);
            }
            ui.set_class(label, "muted", *muted);
            ui.set_inline_style(label, &format!("left: {left}px; top: {top}px"));
            ui.set_visible(label, true);
        }
    }

    /// What pressing and dragging on the timeline asks for this frame.
    pub fn drag(&mut self, ui: &Ui, beats: &[(Beat, BeatMark)]) -> Vec<TimelineEdit> {
        let [left, top, width, _] = ui.rect(self.canvas);
        let Some((px, py)) = ui.pointer_position() else { return Vec::new() };
        let (x, y) = (px - left, py - top);
        if !ui.is_pressed(self.canvas) {
            return match self.held.take() {
                Some(Held::Edge { .. }) | Some(Held::Beat { moved: true, .. }) => vec![TimelineEdit::Done],
                _ => Vec::new(),
            };
        }
        let held = match self.held {
            Some(held) => held,
            None => {
                let held = self.press(width, x, y, beats);
                self.held = Some(held);
                return match held {
                    Held::Playhead => vec![TimelineEdit::Seek(self.t(width, x).max(0.0))],
                    Held::Edge { at, .. } | Held::Beat { at, .. } => vec![TimelineEdit::Pick(Some(at))],
                };
            }
        };
        match held {
            Held::Playhead => vec![TimelineEdit::Seek(self.t(width, x).clamp(0.0, self.span))],
            Held::Edge { at, from, length } => {
                let length = (length + self.t(width, x) - self.t(width, from)).max(0.02);
                vec![TimelineEdit::Retime { at, length }]
            }
            Held::Beat { at, from, moved } => {
                if !moved && (x - from).abs() < SLOP {
                    return Vec::new();
                }
                self.held = Some(Held::Beat { at, from, moved: true });
                vec![TimelineEdit::Shift { at, by: self.t(width, x) - self.t(width, from) }]
            }
        }
    }

    /// What a press at `x`, `y` takes: a beat's block or its right edge,
    /// or else the playhead.
    fn press(&self, width: f32, x: f32, y: f32, beats: &[(Beat, BeatMark)]) -> Held {
        let lane = ((y - RULER - BRACKETS) / LANE).floor();
        if !(0.0..2.0).contains(&lane) {
            return Held::Playhead;
        }
        let hand = if lane < 1.0 { Hand::Right } else { Hand::Left };
        let mut start = 0.0;
        for (at, (beat, _)) in beats.iter().enumerate() {
            let (from, to) = (self.x(width, start), self.x(width, start + beat.length()));
            start += beat.length();
            if beat.hand != hand || x < from || x > to + EDGE {
                continue;
            }
            if (to - x).abs() <= EDGE {
                return Held::Edge { at, from: x, length: beat.length() };
            }
            return Held::Beat { at, from: x, moved: false };
        }
        Held::Playhead
    }

    /// Whether a drag of the timeline is under way.
    pub fn is_dragging(&self) -> bool {
        self.held.is_some()
    }
}

/// An event's name, as the Events lane shows it.
fn event_title(event: Event) -> String {
    match event {
        Event::None => String::new(),
        Event::Eject => "Eject".into(),
        Event::Chamber => "Chamber".into(),
        Event::Unload => "Unload".into(),
        Event::Load => "Load".into(),
        Event::AddRounds(1) => "+1 Round".into(),
        Event::AddRounds(rounds) => format!("+{rounds} Rounds"),
        Event::Open => "Open".into(),
        Event::Close => "Close".into(),
        Event::SeeFeed => "See Feed".into(),
        Event::SeeChamber => "See Chamber".into(),
    }
}

/// A part's name, as the Parts lane and the details show it.
pub(crate) fn part_title(part: PartRole) -> &'static str {
    match part {
        PartRole::Bolt => "Bolt",
        PartRole::Slide => "Slide",
        PartRole::Toggle => "Toggle",
        PartRole::Pump => "Pump",
        PartRole::Magazine => "Magazine",
        PartRole::Clip => "Clip",
        PartRole::Cover => "Cover",
        PartRole::Barrel => "Barrel",
        PartRole::Case => "Case",
    }
}
