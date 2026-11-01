//! The Frame Debugger panel: the passes of the frame the editor just drew,
//! in order with their GPU times; the images the picked pass used, and how;
//! and the picked image as it stood right after that pass, with its mip, its
//! channels and an exposure to choose. Above them, how many objects the
//! frame drew and how many culling left out.
//!
//! The renderer notes the passes only while the panel shows, and copies an
//! image only once one is picked (see `frame_debugger` in voxear_render), so
//! the panel costs nothing when it is closed.

use std::{
    cell::RefCell,
    rc::Rc,
    time::{Duration, Instant},
};

use voxear_render::{Channels, FrameImage, FrameLook, FramePass, Renderer};
use voxear_ui::{ItemFns, NodeId, Style, Ui};

/// How often the lists' times are read again.
const REFRESH: Duration = Duration::from_millis(400);
const ROW: f32 = 22.0;
const CHANNELS: [(&str, Channels); 5] = [
    ("RGB", Channels::Rgb),
    ("R", Channels::Red),
    ("G", Channels::Green),
    ("B", Channels::Blue),
    ("A", Channels::Alpha),
];
/// Depth is held as small numbers, so it starts brighter, to be seen at all.
const DEPTH_EXPOSURE: f32 = 6.0;

/// What the lists show, shared with their rows.
#[derive(Default)]
struct Shown {
    passes: Vec<FramePass>,
    /// The picked pass's images.
    images: Vec<FrameImage>,
}

pub struct FrameDebuggerPanel {
    summary: NodeId,
    passes: NodeId,
    images: NodeId,
    title: NodeId,
    controls: NodeId,
    channels: NodeId,
    mip: NodeId,
    exposure: NodeId,
    exposure_value: NodeId,
    view: NodeId,
    hint: NodeId,
    shown: Rc<RefCell<Shown>>,
    pass: Option<usize>,
    image: Option<usize>,
    /// The mips the mip dropdown offers, so its options change only with them.
    mips: u32,
    read: Option<Instant>,
}

impl FrameDebuggerPanel {
    pub fn new(ui: &mut Ui, root: NodeId) -> Self {
        let bar = ui.panel(root, Style::row());
        ui.add_class(bar, "console-toolbar");
        let summary = ui.add_label(bar, "");
        ui.add_class(summary, "entities-summary");
        let line = ui.panel(root, Style::default());
        ui.add_class(line, "divider");

        let columns = ui.panel(root, Style::row());
        ui.add_class(columns, "fill");
        ui.add_class(columns, "entities-columns");
        let shown: Rc<RefCell<Shown>> = Rc::default();
        let column = |ui: &mut Ui, title: &str, class: &str| {
            let column = ui.panel(columns, Style::default());
            ui.add_class(column, "entities-column");
            ui.add_class(column, class);
            let heading = ui.add_label(column, title);
            ui.add_class(heading, "profiler-heading");
            column
        };

        let left = column(ui, "Passes", "frame-passes");
        let rows = shown.clone();
        let passes = ui.list_view(
            left,
            ROW,
            0,
            ItemFns {
                make: make_row,
                bind: move |ui: &mut Ui, row: NodeId, index: usize| {
                    let shown = rows.borrow();
                    let Some(pass) = shown.passes.get(index) else { return };
                    let ms = pass.ms.map_or(String::new(), |ms| format!("{ms:.3} ms"));
                    fill_row(ui, row, &pass.name, &ms);
                },
            },
        );
        ui.add_class(passes, "fill");

        let middle = column(ui, "Images", "frame-images");
        let rows = shown.clone();
        let images = ui.list_view(
            middle,
            ROW,
            0,
            ItemFns {
                make: make_row,
                bind: move |ui: &mut Ui, row: NodeId, index: usize| {
                    let shown = rows.borrow();
                    let Some(image) = shown.images.get(index) else { return };
                    fill_row(ui, row, &image.name, image.used);
                },
            },
        );
        ui.add_class(images, "fill");

        let right = column(ui, "View", "frame-view");
        let title = ui.add_label(right, "");
        ui.add_class(title, "frame-view-title");
        let controls = ui.panel(right, Style::row());
        ui.add_class(controls, "frame-view-controls");
        let channels = ui.segmented(controls);
        for (name, _) in CHANNELS {
            ui.add_segment(channels, name, None);
        }
        let mip = ui.dropdown(controls, &["Mip 0"], 0);
        ui.add_class(mip, "frame-view-mip");
        let exposure = ui.slider(controls, -10.0, 10.0, 0.0);
        ui.add_class(exposure, "frame-view-exposure");
        let exposure_value = ui.add_label(controls, "");
        ui.add_class(exposure_value, "entities-row-count");
        let view = ui.image(right);
        ui.add_class(view, "frame-view-image");
        let hint = ui.add_label(right, "");
        ui.add_class(hint, "entities-hint");
        let mut panel = Self {
            summary,
            passes,
            images,
            title,
            controls,
            channels,
            mip,
            exposure,
            exposure_value,
            view,
            hint,
            shown,
            pass: None,
            image: None,
            mips: 1,
            read: None,
        };
        panel.show_view(ui, None);
        panel
    }

    /// Reads the frame's passes, follows what was picked, and tells the
    /// renderer what to look at; while the panel is hidden, it tells the
    /// renderer to stop.
    pub fn update(&mut self, ui: &mut Ui, shown: bool, renderer: &mut Renderer) {
        if !shown {
            renderer.set_frame_debugger(false, None);
            self.read = None;
            return;
        }
        let mut picked = false;
        if ui.selection_changed(self.passes) {
            self.pass = ui.selection(self.passes).first().copied();
            self.image = None;
            picked = true;
        }
        if ui.selection_changed(self.images) {
            self.image = ui.selection(self.images).first().copied();
            picked = true;
        }
        let passes = renderer.frame_passes();
        let names_changed = {
            let shown = self.shown.borrow();
            shown.passes.len() != passes.len() || shown.passes.iter().zip(passes).any(|(a, b)| a.name != b.name)
        };
        if names_changed || picked || self.read.is_none_or(|read| read.elapsed() > REFRESH) {
            self.read_passes(ui, renderer, picked);
        }

        let image = self.picked_image();
        if picked {
            self.reset_controls(ui, image.as_ref());
        }
        let exposure = (ui.slider_value(self.exposure) * 2.0).round() / 2.0;
        ui.set_text(self.exposure_value, &format!("{exposure:+.1} stops"));
        let look = match (self.pass, self.image, &image) {
            (Some(pass), Some(image), Some(found)) if found.viewable => Some(FrameLook {
                pass,
                image,
                mip: ui.selected_option(self.mip) as u32,
                channels: CHANNELS[ui.segment(self.channels).min(CHANNELS.len() - 1)].1,
                exposure,
            }),
            _ => None,
        };
        renderer.set_frame_debugger(true, look);
        self.show_view(ui, look.and(renderer.frame_debugger_view()).zip(image));
    }

    /// The frame's passes into their list, and the picked pass's images into
    /// theirs, with the frame's counts above.
    fn read_passes(&mut self, ui: &mut Ui, renderer: &Renderer, picked: bool) {
        self.read = Some(Instant::now());
        let passes = renderer.frame_passes().to_vec();
        if self.pass.is_some_and(|pass| pass >= passes.len()) {
            (self.pass, self.image) = (None, None);
        }
        let images = self.pass.and_then(|pass| passes.get(pass)).map_or(Vec::new(), |pass| pass.images.clone());
        if self.image.is_none() && picked {
            // A pass picked shows what it made: its first image written.
            self.image = images.iter().position(|image| image.writes).or((!images.is_empty()).then_some(0));
            if let Some(image) = self.image {
                ui.set_selection(self.images, &[image]);
            }
        }
        let gpu: f32 = passes.iter().filter_map(|pass| pass.ms).sum();
        let stats = renderer.stats();
        ui.set_text(
            self.summary,
            &format!(
                "{} passes · {gpu:.2} ms on the GPU · {} objects drawn, {} outside the view, {} hidden behind others",
                passes.len(),
                stats.drawn_early + stats.drawn_late,
                stats.frustum_culled,
                stats.occlusion_culled
            ),
        );
        let (pass_count, image_count) = (passes.len(), images.len());
        *self.shown.borrow_mut() = Shown { passes, images };
        ui.set_list_count(self.passes, pass_count);
        ui.set_list_count(self.images, image_count);
        ui.refresh_list(self.passes);
        ui.refresh_list(self.images);
    }

    fn picked_image(&self) -> Option<FrameImage> {
        self.shown.borrow().images.get(self.image?).cloned()
    }

    /// Fits the controls to a newly picked image: a one-channel image shows
    /// its one channel, depth starts brighter, and the mips are its own.
    fn reset_controls(&mut self, ui: &mut Ui, image: Option<&FrameImage>) {
        let Some(image) = image else { return };
        ui.set_segment(self.channels, if image.channels == 1 { 1 } else { 0 });
        ui.set_slider_value(self.exposure, if image.depth { DEPTH_EXPOSURE } else { 0.0 });
        if image.mips != self.mips {
            let options: Vec<String> = (0..image.mips)
                .map(|mip| format!("Mip {mip} · {}×{}", (image.size[0] >> mip).max(1), (image.size[1] >> mip).max(1)))
                .collect();
            ui.set_options(self.mip, &options.iter().map(String::as_str).collect::<Vec<_>>());
            self.mips = image.mips;
        }
        ui.set_selected_option(self.mip, 0);
        ui.set_visible(self.mip, image.mips > 1);
    }

    /// The view, its title and the hint under it.
    fn show_view(&mut self, ui: &mut Ui, view: Option<((voxear_ui::ImageRef, [u32; 2]), FrameImage)>) {
        let picked = self.picked_image();
        let title = match &picked {
            Some(image) => {
                let mips = if image.mips > 1 { format!(" · {} mips", image.mips) } else { String::new() };
                let [width, height] = image.size;
                format!("{} · {width}×{height} · {}{mips}", image.name, image.format)
            }
            None => String::new(),
        };
        ui.set_text(self.title, &title);
        ui.set_visible(self.controls, view.is_some());
        let hint = match (&picked, &view) {
            (None, _) => "Pick a pass, then one of its images, to see it as it stood after the pass.",
            (Some(image), None) if !image.viewable => "This system does not let the window's own image be copied.",
            (Some(_), None) => "The view shows from the next frame.",
            (Some(_), Some(_)) => "",
        };
        ui.set_text(self.hint, hint);
        ui.set_visible(self.hint, !hint.is_empty());
        match view {
            Some(((image, [width, height]), _)) => {
                ui.set_image(self.view, Some(image));
                ui.set_inline_style(self.view, &format!("aspect-ratio: {width} / {height}"));
                ui.set_visible(self.view, true);
            }
            None => {
                ui.set_image(self.view, None);
                ui.set_visible(self.view, false);
            }
        }
    }
}

fn make_row(ui: &mut Ui, row: NodeId) {
    ui.add_class(row, "entities-row");
    let name = ui.add_label(row, "");
    ui.add_class(name, "entities-row-name");
    let detail = ui.add_label(row, "");
    ui.add_class(detail, "entities-row-count");
}

/// Fills a row's two labels.
fn fill_row(ui: &mut Ui, row: NodeId, name: &str, detail: &str) {
    if let [one, two, ..] = ui.children(row)[..] {
        ui.set_text(one, name);
        ui.set_text(two, detail);
    }
}
