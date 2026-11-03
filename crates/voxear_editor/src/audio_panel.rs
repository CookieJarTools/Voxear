//! The Audio panel: every voice the game is playing, as the mixer has it,
//! read again ten times a second while the panel shows. Each row names
//! what plays it, an Obj's Audio Source, a one-shot or the editor's
//! preview, with its sound, bus, volume, how loud it is heard and from how
//! far, how much gets past the walls, and whether it is paused or ending.
//! The line along the top counts them and says whether headphones and the
//! room's reverb are on, and how long the room rings, or that its echoes
//! play by convolution. Clicking a source's row selects its Obj.

use std::{
    cell::RefCell,
    rc::Rc,
    time::{Duration, Instant},
};

use voxear_ecs::Entity;
use voxear_engine::{
    Game,
    audio::{HeardVoice, ReverbMode},
};
use voxear_ui::{ItemFns, NodeId, Style, Ui};

use crate::columns::{self, TableHeader};

/// How often the voices are read again while the panel shows.
const REFRESH: Duration = Duration::from_millis(100);
const ROW: f32 = 22.0;
/// The columns, each with the share of the width it starts with, and the
/// class of its cells; the last takes the rest.
const COLUMNS: [(&str, f32, &str); 8] = [
    ("Voice", 0.18, "entities-row-name"),
    ("Sound", 0.22, "audio-sound"),
    ("Bus", 0.08, "audio-bus"),
    ("Volume", 0.08, "entities-row-count"),
    ("Heard", 0.09, "entities-row-count"),
    ("Distance", 0.09, "entities-row-count"),
    ("Through", 0.09, "entities-row-count"),
    ("State", 0.0, "audio-state"),
];

pub struct AudioPanel {
    summary: NodeId,
    list: NodeId,
    hint: NodeId,
    header: TableHeader,
    rows: Rc<RefCell<Vec<NodeId>>>,
    shown: Rc<RefCell<Vec<[String; 8]>>>,
    sources: Vec<Option<Entity>>,
    read: Option<Instant>,
    watching: bool,
}

impl AudioPanel {
    pub fn new(ui: &mut Ui, root: NodeId) -> Self {
        let bar = ui.panel(root, Style::row());
        ui.add_class(bar, "console-toolbar");
        let summary = ui.add_label(bar, "");
        ui.add_class(summary, "entities-summary");
        let line = ui.panel(root, Style::default());
        ui.add_class(line, "divider");

        let body = ui.panel(root, Style::default());
        ui.add_class(body, "fill");
        ui.add_class(body, "entities-column");
        let hint = ui.add_label(body, "No sound is playing. Press Play to hear the game.");
        ui.add_class(hint, "entities-hint");
        let headings: Vec<(&str, f32)> = COLUMNS.iter().map(|&(title, width, _)| (title, width)).collect();
        let header = TableHeader::new(ui, body, &headings);
        let shown: Rc<RefCell<Vec<[String; 8]>>> = Rc::default();
        let (widths, made) = (header.widths(), Rc::new(RefCell::new(Vec::new())));
        let (texts, kept) = (shown.clone(), made.clone());
        let list = ui.list_view(
            body,
            ROW,
            0,
            ItemFns {
                make: move |ui: &mut Ui, row: NodeId| {
                    ui.add_class(row, "entities-row");
                    ui.add_class(row, "table-row");
                    for &(_, _, class) in &COLUMNS {
                        let label = ui.add_label(row, "");
                        ui.add_class(label, class);
                    }
                    columns::fit(ui, row, &widths.borrow());
                    kept.borrow_mut().push(row);
                },
                bind: move |ui: &mut Ui, row: NodeId, index: usize| {
                    let texts = texts.borrow();
                    let Some(cells) = texts.get(index) else { return };
                    for (label, text) in ui.children(row).into_iter().zip(cells) {
                        ui.set_text(label, text);
                    }
                },
            },
        );
        ui.add_class(list, "fill");
        Self { summary, list, hint, header, rows: made, shown, sources: Vec::new(), read: None, watching: false }
    }

    /// Reads the voices again while the panel shows, and has the mixer stop
    /// saying what it plays once it does not. Gives the Obj of a source's
    /// row when one is clicked.
    pub fn update(&mut self, ui: &mut Ui, shown: bool, game: &Game) -> Option<Entity> {
        if self.watching != shown {
            self.watching = shown;
            game.audio.audio.watch_voices(shown);
        }
        if !shown {
            return None;
        }
        if self.header.update(ui) {
            let widths = self.header.widths();
            for &row in self.rows.borrow().iter() {
                columns::fit(ui, row, &widths.borrow());
            }
        }
        let picked = ui.selection_changed(self.list).then(|| ui.selection(self.list).first().copied()).flatten();
        if self.read.is_none_or(|read| read.elapsed() >= REFRESH) {
            self.read = Some(Instant::now());
            self.show(ui, game);
        }
        picked.and_then(|index| self.sources.get(index).copied().flatten())
    }

    pub fn list(&self) -> NodeId {
        self.list
    }

    pub fn summary(&self) -> NodeId {
        self.summary
    }

    fn show(&mut self, ui: &mut Ui, game: &Game) {
        let voices = game.audio.heard_voices(&game.engine.world, &game.content);
        let listener = game.audio.listener_at();
        let mut parts = vec![match voices.len() {
            1 => "1 voice".to_string(),
            count => format!("{count} voices"),
        }];
        if game.audio.audio.headphones() {
            parts.push("Headphones".into());
        }
        if game.audio.reverb_on() && game.audio.reverb_mode() == ReverbMode::Convolution {
            parts.push("Convolution reverb".into());
        } else if game.audio.reverb_on() {
            parts.push(format!("Reverb {:.1} s", game.audio.room().0[1]));
        }
        ui.set_text(self.summary, &parts.join("   "));
        ui.set_visible(self.hint, voices.is_empty());
        self.sources = voices.iter().map(|voice| voice.source).collect();
        *self.shown.borrow_mut() = voices.iter().map(|voice| cells(voice, listener)).collect();
        ui.set_list_count(self.list, voices.len());
        ui.refresh_list(self.list);
    }
}

/// A row's texts, a column each.
fn cells(voice: &HeardVoice, listener: Option<voxear_core::math::Vec3>) -> [String; 8] {
    let report = &voice.report;
    let sound = voice.sound.rsplit('/').next().unwrap_or_default().to_string();
    let distance = match (report.at, listener) {
        (Some(at), Some(listener)) => format!("{:.1} m", at.distance(listener)),
        (None, _) => "2D".to_string(),
        _ => String::new(),
    };
    // The middle band stands for the rest: where most of a voice is.
    let through = report.muffle.map_or_else(|| "Open".to_string(), |muffle| format!("{:.0}%", muffle.0[1] * 100.0));
    let mut state = vec![if report.ending {
        "Ending"
    } else if report.paused {
        "Paused"
    } else {
        "Playing"
    }];
    if report.looping {
        state.push("Loop");
    }
    if report.binaural {
        state.push("HRTF");
    }
    [
        voice.name.clone(),
        sound,
        format!("{:?}", report.bus),
        format!("{:.2}", report.volume),
        decibels(report.heard),
        distance,
        through,
        state.join(" · "),
    ]
}

/// How loud, in decibels from full.
fn decibels(level: f32) -> String {
    if level <= 1e-5 { "−∞ dB".to_string() } else { format!("{:.0} dB", 20.0 * level.log10()) }
}
