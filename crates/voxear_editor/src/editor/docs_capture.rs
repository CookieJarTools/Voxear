//! Pictures and recordings for the VoxDocs page being written. Capture Scene
//! takes the Scene view as it is; Record Scene and Record Game take three
//! seconds of the Scene or Game view as a GIF, fifteen frames a second. The
//! renderer copies the view before the UI draws over it, so the editor's
//! overlays are not in them. When VoxDocs shares its place with the view,
//! the view shows for as long as that takes, and the page comes back with
//! the picture at the cursor. A recording is made into a GIF off the main
//! thread, no wider than 640 pixels.

use std::{
    io::Cursor,
    sync::mpsc::{self, Receiver, TryRecvError},
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use image::{
    Delay, Frame, RgbaImage,
    codecs::gif::{GifEncoder, Repeat},
    imageops::FilterType,
};
use voxear_docs::Capture;
use voxear_render::{CapturedImage, Renderer};
use voxear_ui::NodeId;

use super::Editor;

/// A recording's frames a second, how long it runs, and the widest a GIF
/// is made.
const FPS: u32 = 15;
const SECONDS: u32 = 3;
const GIF_WIDTH: u32 = 640;
/// Frames to wait once a view is shown, for it to be laid out and drawn.
const SETTLE: u32 = 3;
/// How long to wait for a copy before taking it that none will come.
const PATIENCE: Duration = Duration::from_secs(3);

/// A picture or recording under way.
pub(super) struct Taking {
    capture: Capture,
    view: NodeId,
    /// The view's tab was shown for it, so VoxDocs comes back after.
    switched: bool,
    settle: u32,
    /// When the first frame was asked for, how many have been, and those
    /// come back.
    began: Option<Instant>,
    asked: u32,
    frames: Vec<CapturedImage>,
    /// The GIF being made from the frames.
    encoding: Option<Receiver<Result<Vec<u8>>>>,
}

impl Editor {
    pub(super) fn update_docs_capture(&mut self, renderer: &mut Renderer) {
        if let Some(capture) = self.docs.take_capture_request()
            && self.docs_capture.is_none()
        {
            self.start_capture(capture);
        }
        let Some(mut taking) = self.docs_capture.take() else { return };
        while let Some(image) = renderer.take_capture() {
            taking.frames.push(image);
        }
        match self.go_on_capturing(&mut taking, renderer) {
            Ok(true) => self.docs_capture = Some(taking),
            Ok(false) => self.end_capture(&taking),
            Err(error) => {
                self.notify(&format!("{error:#}"));
                self.end_capture(&taking);
            }
        }
    }

    /// Shows the view to take, when VoxDocs hides it.
    fn start_capture(&mut self, capture: Capture) {
        let game = capture == Capture::GameRecording;
        if game && self.game_camera().is_none() {
            return self.notify("The level has no camera for the Game view to look through");
        }
        let (view, panel) = match game {
            true => (self.nodes.game_viewport, "game".to_string()),
            false => (self.scene.viewport, self.scene.panel.clone()),
        };
        let switched = !self.shown(view);
        if switched {
            self.show_panel(&panel);
        }
        let taking = Taking {
            capture,
            view,
            switched,
            settle: SETTLE,
            began: None,
            asked: 0,
            frames: Vec::new(),
            encoding: None,
        };
        self.docs_capture = Some(taking);
    }

    /// Asks for this frame's copy, keeps what is done, and says whether it
    /// is still under way.
    fn go_on_capturing(&mut self, taking: &mut Taking, renderer: &mut Renderer) -> Result<bool> {
        if taking.settle > 0 {
            taking.settle -= 1;
            return Ok(true);
        }
        if let Some(encoding) = &taking.encoding {
            return match encoding.try_recv() {
                Ok(gif) => {
                    let name = if taking.capture == Capture::GameRecording { "Game" } else { "Scene" };
                    self.keep(name, "gif", &gif?)?;
                    Ok(false)
                }
                Err(TryRecvError::Empty) => Ok(true),
                Err(TryRecvError::Disconnected) => anyhow::bail!("Making the GIF stopped"),
            };
        }
        let wanted = if taking.capture == Capture::Scene { 1 } else { FPS * SECONDS };
        let began = *taking.began.get_or_insert_with(Instant::now);
        let due = (began.elapsed().as_secs_f32() * FPS as f32) as u32 + 1;
        if taking.asked < wanted.min(due) {
            let area = self.capture_area(taking.view);
            renderer.request_capture(voxear_render::Capture { area: Some(area), before_ui: true });
            taking.asked += 1;
        }
        if (taking.frames.len() as u32) < wanted {
            let late = began.elapsed() > Duration::from_secs(u64::from(SECONDS)) + PATIENCE;
            anyhow::ensure!(!late, "The view could not be copied on this system");
            return Ok(true);
        }
        if taking.capture == Capture::Scene {
            let png = png(&taking.frames[0])?;
            self.keep("Scene", "png", &png)?;
            return Ok(false);
        }
        let frames = std::mem::take(&mut taking.frames);
        let (done, encoding) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = done.send(gif(frames));
        });
        taking.encoding = Some(encoding);
        self.notify("Making the GIF");
        Ok(true)
    }

    /// The view's area in the window's pixels, as the renderer copies it.
    fn capture_area(&self, view: NodeId) -> [u32; 4] {
        let [_, _, width, height] = self.ui.rect(self.ui.root());
        let size = ((width * self.scale) as u32, (height * self.scale) as u32);
        self.viewport_rect(Some(view), size)
    }

    /// Keeps a picture with the page being written, at the cursor.
    fn keep(&mut self, name: &str, extension: &str, bytes: &[u8]) -> Result<()> {
        let kept = self.docs.keep_picture(name, extension, bytes).context("keeping the picture")?;
        self.notify(&format!("Kept {kept} with the page"));
        Ok(())
    }

    /// Back to the page, if its place was taken for the view.
    fn end_capture(&mut self, taking: &Taking) {
        if taking.switched {
            self.show_panel("docs");
        }
    }
}

/// A capture as a PNG file's bytes.
fn png(capture: &CapturedImage) -> Result<Vec<u8>> {
    let picture =
        RgbaImage::from_raw(capture.width, capture.height, capture.rgba.clone()).context("the capture's size")?;
    let mut bytes = Cursor::new(Vec::new());
    picture.write_to(&mut bytes, image::ImageFormat::Png).context("making the PNG")?;
    Ok(bytes.into_inner())
}

/// Captures as a looping GIF's bytes, each frame no wider than
/// [`GIF_WIDTH`].
fn gif(frames: Vec<CapturedImage>) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    {
        let mut encoder = GifEncoder::new_with_speed(&mut bytes, 10);
        encoder.set_repeat(Repeat::Infinite).context("making the GIF")?;
        let delay = Delay::from_numer_denom_ms(1000, FPS);
        for capture in frames {
            let picture =
                RgbaImage::from_raw(capture.width, capture.height, capture.rgba).context("a capture's size")?;
            let picture = if picture.width() > GIF_WIDTH {
                let height = (picture.height() * GIF_WIDTH / picture.width()).max(1);
                image::imageops::resize(&picture, GIF_WIDTH, height, FilterType::Triangle)
            } else {
                picture
            };
            encoder.encode_frame(Frame::from_parts(picture, 0, 0, delay)).context("making the GIF")?;
        }
    }
    Ok(bytes)
}
