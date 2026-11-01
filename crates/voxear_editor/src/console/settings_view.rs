//! The Console's settings, in a screen of their own with a live sample, as
//! Sifter has them. Nearly every setting is about how something reads, and
//! a sentence describing a colour or a row height is no use next to seeing
//! it, so the sample is drawn by the Console's own row and stack code: what
//! it previews is what the Console shows.

use std::path::Path;

use tracing::Level;
use voxear_core::logging::LogLine;
use voxear_ui::{NodeId, Style, Ui};

use super::{CLASS, FILE, METHOD, NAMESPACE, PALETTE, PARAMETERS, entry::Entry};
use crate::settings::Settings;

/// The row heights and how many lines are held, as the choices name them.
pub(super) const HEIGHTS: [f32; 3] = [18.0, 20.0, 24.0];
const KEPT: [usize; 4] = [1_000, 5_000, 10_000, 50_000];

/// The parts that take a colour: the settings' name for each, what the
/// screen calls it, and the colour it has until one is picked. Errors and
/// warnings keep the theme's own until then.
pub(super) const PARTS: [(&str, &str, Option<u32>); 7] = [
    ("error", "Errors", None),
    ("warning", "Warnings", None),
    ("namespace", "Namespaces", Some(NAMESPACE)),
    ("class", "Classes", Some(CLASS)),
    ("method", "Methods", Some(METHOD)),
    ("parameters", "Parameters", Some(PARAMETERS)),
    ("file", "Files and lines", Some(FILE)),
];

/// A setting shown as a row of choices.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Choice {
    RowHeight,
    Tint,
    Stripes,
    Category,
    Namespaces,
    Parameters,
    MineOnly,
    Kept,
    ClearOnPlay,
}

/// The sections of choices, in order; the colours come after them.
const SECTIONS: [(&str, &[Choice]); 3] = [
    ("Rows", &[Choice::RowHeight, Choice::Tint, Choice::Stripes, Choice::Category]),
    ("Stack traces", &[Choice::Namespaces, Choice::Parameters, Choice::MineOnly]),
    ("Keeping lines", &[Choice::Kept, Choice::ClearOnPlay]),
];

impl Choice {
    fn title(self) -> &'static str {
        match self {
            Choice::RowHeight => "Row height",
            Choice::Tint => "Colour by kind",
            Choice::Stripes => "Alternating rows",
            Choice::Category => "Show category",
            Choice::Namespaces => "Namespaces",
            Choice::Parameters => "Parameters",
            Choice::MineOnly => "My code only",
            Choice::Kept => "Lines held",
            Choice::ClearOnPlay => "Clear on play",
        }
    }

    fn note(self) -> &'static str {
        match self {
            Choice::RowHeight => "How much room each line gets in the list.",
            Choice::Tint => "Give a whole warning or error line its colour, not only its mark.",
            Choice::Stripes => "Shade every other line, so the eye does not slide from one line to the next.",
            Choice::Category => "How a line says which category it is in: its name, or a dot in its colour.",
            Choice::Namespaces => "Write each frame's namespace in front of its class. They take most of the width.",
            Choice::Parameters => "Write the types each method takes.",
            Choice::MineOnly => "Fold away the frames that are not the project's own into one line to open.",
            Choice::Kept => "How many lines to hold before the oldest go.",
            Choice::ClearOnPlay => "Start each play with an empty Console.",
        }
    }

    fn names(self) -> &'static [&'static str] {
        match self {
            Choice::RowHeight => &["Short", "Cosy", "Tall"],
            Choice::Category => &["Off", "Dot", "Name"],
            Choice::Namespaces | Choice::Parameters => &["Hidden", "Shown"],
            Choice::Kept => &["1,000", "5,000", "10,000", "50,000"],
            _ => &["Off", "On"],
        }
    }

    fn get(self, settings: &Settings) -> usize {
        match self {
            Choice::RowHeight => HEIGHTS.iter().position(|&h| h == settings.console_row_height).unwrap_or(1),
            Choice::Tint => usize::from(settings.console_tint),
            Choice::Stripes => usize::from(settings.console_stripes),
            Choice::Category => settings.console_category.min(2),
            Choice::Namespaces => usize::from(settings.console_namespaces),
            Choice::Parameters => usize::from(settings.console_parameters),
            Choice::MineOnly => usize::from(settings.console_mine_only),
            Choice::Kept => KEPT.iter().position(|&k| k == settings.console_kept).unwrap_or(2),
            Choice::ClearOnPlay => usize::from(settings.clear_console_on_play),
        }
    }

    fn set(self, settings: &mut Settings, at: usize) {
        let on = at == 1;
        match self {
            Choice::RowHeight => settings.console_row_height = HEIGHTS[at],
            Choice::Tint => settings.console_tint = on,
            Choice::Stripes => settings.console_stripes = on,
            Choice::Category => settings.console_category = at,
            Choice::Namespaces => settings.console_namespaces = on,
            Choice::Parameters => settings.console_parameters = on,
            Choice::MineOnly => settings.console_mine_only = on,
            Choice::Kept => settings.console_kept = KEPT[at],
            Choice::ClearOnPlay => settings.clear_console_on_play = on,
        }
    }
}

/// What a click on the screen asks of the Console.
pub(super) enum Change {
    /// A setting changed, so the Console and the sample are drawn again.
    Settings,
    /// A category's colour, kept with the project; `None` for none.
    Category(String, Option<u32>),
    ClearCategories,
    Done,
}

/// A row of swatches: the colour each picks, `None` for the part's own.
type Swatches = Vec<(NodeId, Option<u32>)>;

pub(super) struct SettingsView {
    pub screen: NodeId,
    done: NodeId,
    choices: Vec<(Choice, Vec<NodeId>)>,
    parts: Vec<(&'static str, Swatches)>,
    reset_colours: NodeId,
    categories: Vec<(String, Swatches)>,
    clear_categories: NodeId,
    /// The sample's lines, its stack, and what they show.
    pub sample_rows: Vec<NodeId>,
    pub sample_trace: NodeId,
    pub samples: Vec<Entry>,
}

impl SettingsView {
    /// Builds the screen in `root`, over the Console's list. `categories`
    /// are those the Console has seen, with the colour each has, if any.
    pub fn new(
        ui: &mut Ui,
        root: NodeId,
        project: &Path,
        settings: &Settings,
        categories: &[(String, Option<u32>)],
    ) -> Self {
        let screen = ui.panel(root, Style::column());
        ui.add_class(screen, "fill");
        ui.add_class(screen, "console-settings");
        // The title and Done stay put while what is under them scrolls.
        let head = ui.panel(screen, Style::row());
        ui.add_class(head, "console-settings-head");
        let title = ui.add_label(head, "Console Settings");
        ui.add_class(title, "console-settings-title");
        let done = ui.button(head, "Done");
        ui.add_class(done, "console-button");
        let body = ui.panel(screen, Style::column().scroll());
        ui.add_class(body, "fill");
        ui.add_class(body, "console-settings-body");

        let sample = section(ui, body, "Sample");
        let list = ui.panel(sample, Style::column());
        ui.add_class(list, "console");
        ui.add_class(list, "console-sample");
        let samples = samples(project);
        let sample_rows = samples
            .iter()
            .map(|_| {
                let row = ui.panel(list, Style::row());
                ui.add_class(row, "list-row");
                super::make_row(ui, row);
                row
            })
            .collect();
        let sample_trace = ui.rich_label(sample, &voxear_ui::RichText::new());
        ui.add_class(sample_trace, "console-trace");
        ui.add_class(sample_trace, "console-sample-trace");

        let mut choices = Vec::new();
        for (name, shown) in SECTIONS {
            let section = section(ui, body, name);
            for &choice in shown {
                let control = option(ui, section, choice.title(), choice.note());
                let buttons = choice.names().iter().map(|name| segment(ui, control, name)).collect();
                choices.push((choice, buttons));
            }
        }

        let colours = section(ui, body, "Colours");
        let parts = PARTS
            .iter()
            .map(|&(key, title, own)| {
                let control = option(ui, colours, title, "");
                (key, swatches(ui, control, own))
            })
            .collect();
        let reset_colours = ui.button(colours, "Reset the colours");
        ui.add_class(reset_colours, "console-button");
        ui.add_class(reset_colours, "console-settings-reset");

        let shared = section(ui, body, "Category Colours");
        let note = ui.add_label(
            shared,
            "Kept with the project, in Settings/Console.toml, so the whole team sees the same colours.",
        );
        ui.add_class(note, "console-settings-note");
        let categories = categories
            .iter()
            .map(|(name, _)| {
                let control = option(ui, shared, name, "");
                (name.clone(), swatches(ui, control, None))
            })
            .collect();
        let clear_categories = ui.button(shared, "Clear the category colours");
        ui.add_class(clear_categories, "console-button");
        ui.add_class(clear_categories, "console-settings-reset");

        let view = Self {
            screen,
            done,
            choices,
            parts,
            reset_colours,
            categories,
            clear_categories,
            sample_rows,
            sample_trace,
            samples,
        };
        view.show(ui, settings, &[]);
        view
    }

    /// Picks out each setting's choice and colour as they are now.
    pub fn show(&self, ui: &mut Ui, settings: &Settings, category_colours: &[(String, Option<u32>)]) {
        for (choice, buttons) in &self.choices {
            let at = choice.get(settings);
            for (index, &button) in buttons.iter().enumerate() {
                ui.set_checked(button, index == at);
            }
        }
        for (key, swatches) in &self.parts {
            let picked = settings.console_colours.get(*key).copied();
            pick(ui, swatches, picked);
        }
        for (name, swatches) in &self.categories {
            let picked = category_colours.iter().find(|(n, _)| n == name).and_then(|(_, c)| *c);
            pick(ui, swatches, picked);
        }
    }

    /// What was clicked, if anything, applied to `settings` when it is one.
    pub fn update(&self, ui: &Ui, settings: &mut Settings) -> Option<Change> {
        if ui.clicked(self.done) {
            return Some(Change::Done);
        }
        for (choice, buttons) in &self.choices {
            if let Some(at) = buttons.iter().position(|&b| ui.clicked(b)) {
                choice.set(settings, at);
                return Some(Change::Settings);
            }
        }
        for (key, swatches) in &self.parts {
            if let Some(&(_, colour)) = swatches.iter().find(|(b, _)| ui.clicked(*b)) {
                match colour {
                    Some(colour) => settings.console_colours.insert(key.to_string(), colour),
                    None => settings.console_colours.remove(*key),
                };
                return Some(Change::Settings);
            }
        }
        if ui.clicked(self.reset_colours) {
            settings.console_colours.clear();
            return Some(Change::Settings);
        }
        for (name, swatches) in &self.categories {
            if let Some(&(_, colour)) = swatches.iter().find(|(b, _)| ui.clicked(*b)) {
                return Some(Change::Category(name.clone(), colour));
            }
        }
        ui.clicked(self.clear_categories).then_some(Change::ClearCategories)
    }
}

/// A section's title, and the column its options go in.
fn section(ui: &mut Ui, screen: NodeId, title: &str) -> NodeId {
    let heading = ui.add_label(screen, title);
    ui.add_class(heading, "console-settings-section");
    let column = ui.panel(screen, Style::column());
    ui.add_class(column, "console-settings-box");
    column
}

/// An option: its title and what it does on the left, and the row its
/// control goes in on the right.
fn option(ui: &mut Ui, section: NodeId, title: &str, note: &str) -> NodeId {
    let row = ui.panel(section, Style::row());
    ui.add_class(row, "console-option");
    let words = ui.panel(row, Style::column());
    ui.add_class(words, "console-option-words");
    let name = ui.add_label(words, title);
    ui.add_class(name, "console-option-title");
    if !note.is_empty() {
        let note = ui.add_label(words, note);
        ui.add_class(note, "console-option-note");
    }
    let control = ui.panel(row, Style::row());
    ui.add_class(control, "console-option-control");
    control
}

fn segment(ui: &mut Ui, control: NodeId, name: &str) -> NodeId {
    let button = ui.button(control, name);
    ui.add_class(button, "console-segment");
    button
}

/// A swatch for each colour of the palette, after one for the part's own
/// colour: `own`, or the theme's when there is none.
fn swatches(ui: &mut Ui, control: NodeId, own: Option<u32>) -> Swatches {
    let mut swatches = Vec::new();
    let default = ui.button(control, "");
    ui.add_class(default, "console-swatch");
    ui.add_class(default, "console-swatch-own");
    if let Some(own) = own {
        ui.set_inline_style(default, &format!("background-color: #{own:06x}"));
    }
    swatches.push((default, None));
    for (_, colour) in PALETTE {
        let swatch = ui.button(control, "");
        ui.add_class(swatch, "console-swatch");
        ui.set_inline_style(swatch, &format!("background-color: #{colour:06x}"));
        swatches.push((swatch, Some(colour)));
    }
    swatches
}

fn pick(ui: &mut Ui, swatches: &Swatches, picked: Option<u32>) {
    for &(swatch, colour) in swatches {
        ui.set_checked(swatch, colour == picked);
    }
}

/// The lines the sample shows: a script's line and its stack, a warning
/// from the engine, and an error.
fn samples(project: &Path) -> Vec<Entry> {
    let script = project.join("Scripts").join("Spinner.cs");
    let mut said = LogLine::new(Level::INFO, "Spinner A checked in with the GameManager");
    said.target = "script".into();
    said.seconds = 72.4;
    said.stack = Some(format!(
        "   at Hello.Spinner.Start() in {}:line 31\n   at Voxear.Scripting.GameScripts.Dispatch(Phase phase, Int32 \
         batch) in /voxear/managed/Voxear.Core/Scripting/GameScripts.cs:line 88\n   at \
         Voxear.Scripting.EntryPoints.Tick(Int32 phase) in /voxear/managed/Voxear.Core/Scripting/EntryPoints.cs:line 40",
        script.display()
    ));
    let mut warned = LogLine::new(Level::WARN, "Lamp has no bulb for Pulse to breathe");
    warned.target = "voxear_engine::levels".into();
    warned.file = Some("crates/voxear_engine/src/levels.rs".into());
    warned.line = Some(214);
    warned.seconds = 73.0;
    let mut failed = LogLine::new(Level::ERROR, "[Combat] The boss has no health bar");
    failed.target = "script".into();
    failed.seconds = 75.8;
    [said, warned, failed].iter().map(|line| Entry::new(line, project)).collect()
}
