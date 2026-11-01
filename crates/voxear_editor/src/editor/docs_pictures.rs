//! Pictures in VoxDocs pages: each file a page shows is read, made smaller
//! if it is very large, and uploaded once for the rest of the session, a
//! couple a frame so a page full of them does not stall the editor.
//!
//! **Live assets**: a picture that names a prefab, model or material shows
//! its preview, as the Project panel draws them, and changes when the asset
//! does. Previews share an atlas whose cells go to whichever were asked for
//! lately, so each live picture is asked for every frame, and handed to the
//! page again should it be drawn somewhere else. A live material shows its
//! fields under it as well, to change right there, as in the Inspector.
//!
//! **GIFs** play: each frame is uploaded once, and the page is handed the
//! one due, by its delay, which it swaps in where the picture shows.

use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use image::imageops::FilterType;
use voxear_render::{Renderer, TextureSource};
use voxear_ui::{ImageRef, NodeId};

use super::Editor;
use crate::{
    asset_inspector::{AssetAction, AssetView},
    project_view::{ItemKind, ProjectItem},
};

/// Pictures read in one frame.
const PER_FRAME: usize = 2;
/// A live asset's preview's size in pixels, and how many frames it may take
/// to draw before it is taken to have nothing to show.
const LIVE_SIZE: u32 = 256;
const LIVE_PATIENCE: u32 = 240;
/// The kinds of asset a page can show live, by their extensions.
const LIVE_KINDS: [&str; 5] = ["prefab", "material", "gltf", "glb", "fbx"];

/// A live asset a page shows: its file, the asset, the preview last handed
/// to the page, and how many frames it has waited for one.
pub(super) struct LiveAsset {
    path: PathBuf,
    reference: String,
    shown: Option<ImageRef>,
    waited: u32,
}
/// The longest side a picture is uploaded at; a page never shows it larger.
const MAX_SIDE: u32 = 2048;
/// The most frames of a GIF kept, and the shortest a frame shows, as
/// browsers keep a GIF that asks for less.
const MOST_FRAMES: usize = 300;
const SHORTEST_FRAME: Duration = Duration::from_millis(20);

/// A GIF a page shows: its file, each frame with how long it shows, its
/// size, when it began, and the frame last handed to the page.
pub(super) struct Animation {
    path: PathBuf,
    frames: Vec<(ImageRef, Duration)>,
    size: [u32; 2],
    began: Instant,
    shown: usize,
}

impl Editor {
    pub(super) fn update_docs_pictures(&mut self, renderer: &mut Renderer) {
        for path in self.docs.wanted_pictures(PER_FRAME) {
            if let Some(reference) = live_reference(&path, &self.game.project.root) {
                self.live_assets.push(LiveAsset { path, reference, shown: None, waited: 0 });
                continue;
            }
            let name = path.file_name().unwrap_or_default().to_string_lossy().into_owned();
            let upload = |renderer: &mut Renderer, (size, rgba): ([u32; 2], Vec<u8>)| -> Result<ImageRef> {
                let source = TextureSource { name: &name, width: size[0], height: size[1], rgba: &rgba, srgb: true };
                Ok(ImageRef { texture: renderer.add_texture(&source)?.0, uv: [0.0, 0.0, 1.0, 1.0] })
            };
            let gif = path.extension().is_some_and(|e| e.eq_ignore_ascii_case("gif"));
            let shown = if gif {
                read_animation(&path)
                    .and_then(|(size, frames)| {
                        let frames: Result<Vec<(ImageRef, Duration)>> = frames
                            .into_iter()
                            .map(|(rgba, delay)| Ok((upload(renderer, (size, rgba))?, delay)))
                            .collect();
                        Ok((size, frames?))
                    })
                    .map(|(size, frames)| {
                        let first = frames[0].0;
                        self.animations.push(Animation {
                            path: path.clone(),
                            frames,
                            size,
                            began: Instant::now(),
                            shown: 0,
                        });
                        (first, size)
                    })
            } else {
                read_picture(&path).and_then(|(size, rgba)| Ok((upload(renderer, (size, rgba))?, size)))
            };
            let shown =
                shown.inspect_err(|error| tracing::warn!("VoxDocs: could not show {}: {error:#}", path.display()));
            self.docs.set_picture(&mut self.ui, &path, shown.ok());
        }
        self.update_live_assets(renderer);
        self.update_animations();
        self.update_live_fields(renderer);
    }

    /// Hands each GIF's page the frame now due, when it changes.
    fn update_animations(&mut self) {
        for animation in &mut self.animations {
            let total: Duration = animation.frames.iter().map(|(_, delay)| *delay).sum();
            let mut into =
                Duration::from_nanos((animation.began.elapsed().as_nanos() % total.as_nanos().max(1)) as u64);
            let mut due = 0;
            while due + 1 < animation.frames.len() && into >= animation.frames[due].1 {
                into -= animation.frames[due].1;
                due += 1;
            }
            if due != animation.shown {
                animation.shown = due;
                self.docs.set_picture(&mut self.ui, &animation.path, Some((animation.frames[due].0, animation.size)));
            }
        }
    }

    /// Fills the places the page leaves for live materials' fields, each time
    /// it is drawn again, and saves what is changed in them as the Inspector
    /// does, which draws the preview again.
    fn update_live_fields(&mut self, renderer: &mut Renderer) {
        let places = self.docs.live_fields();
        let built: Vec<NodeId> = self.live_fields.iter().map(|(place, _)| *place).collect();
        if places.iter().map(|(_, place)| *place).ne(built.iter().copied()) {
            let root = self.game.project.root.clone();
            self.live_fields = places
                .into_iter()
                .filter_map(|(path, place)| {
                    let item = project_item(&path, &root)?;
                    Some((place, AssetView::fields(&mut self.ui, place, &item, &self.game)?))
                })
                .collect();
        }
        let ui = &mut self.ui;
        let actions: Vec<AssetAction> = self.live_fields.iter_mut().filter_map(|(_, view)| view.update(ui)).collect();
        for action in actions {
            self.asset_action(action, renderer);
        }
    }

    /// Asks for each live asset's preview, so it stays drawn, and hands it to
    /// the page when it first is, or moves.
    fn update_live_assets(&mut self, renderer: &mut Renderer) {
        let Editor { live_assets, asset_previews, game, docs, ui, .. } = self;
        for live in live_assets.iter_mut() {
            let image = asset_previews.image(&mut game.content, renderer, &live.reference, LIVE_SIZE);
            let image = image.map(|image| ImageRef { texture: image.texture.0, uv: image.uv });
            match image {
                Some(image) if live.shown != Some(image) => {
                    live.shown = Some(image);
                    docs.set_picture(ui, &live.path, Some((image, [LIVE_SIZE; 2])));
                }
                None if live.shown.is_none() => {
                    live.waited += 1;
                    if live.waited == LIVE_PATIENCE {
                        docs.set_picture(ui, &live.path, None);
                    }
                }
                _ => {}
            }
        }
    }
}

/// The asset a picture shows live, by its path in the project, when the
/// file is a prefab, model or material in it.
fn live_reference(path: &Path, root: &Path) -> Option<String> {
    let extension = path.extension()?.to_string_lossy().to_lowercase();
    if !LIVE_KINDS.contains(&extension.as_str()) {
        return None;
    }
    let within = path.strip_prefix(root).ok()?;
    Some(within.to_string_lossy().replace('\\', "/"))
}

/// A file in the project as the Project panel lists it.
fn project_item(path: &Path, root: &Path) -> Option<ProjectItem> {
    let kind = voxear_assets::AssetKind::from_path(path)?;
    Some(ProjectItem {
        path: path.strip_prefix(root).ok()?.to_string_lossy().replace('\\', "/"),
        name: path.file_name()?.to_string_lossy().into_owned(),
        kind: ItemKind::Asset(kind),
        size: std::fs::metadata(path).map_or(0, |m| m.len()),
    })
}

/// A GIF's frames, each as premultiplied RGBA with how long it shows.
type Frames = Vec<(Vec<u8>, Duration)>;

/// A GIF's frames and their size, made smaller as a picture is when very
/// large.
fn read_animation(path: &Path) -> Result<([u32; 2], Frames)> {
    use image::AnimationDecoder;
    let file =
        std::io::BufReader::new(std::fs::File::open(path).with_context(|| format!("reading {}", path.display()))?);
    let decoder = image::codecs::gif::GifDecoder::new(file).with_context(|| format!("reading {}", path.display()))?;
    let mut frames = Vec::new();
    let mut size = [0, 0];
    for frame in decoder.into_frames().take(MOST_FRAMES) {
        let frame = frame.with_context(|| format!("reading {}", path.display()))?;
        let delay = Duration::from(frame.delay()).max(SHORTEST_FRAME);
        let picture = image::DynamicImage::ImageRgba8(frame.into_buffer());
        let (rgba, frame_size) = shown_rgba(picture);
        size = frame_size;
        frames.push((rgba, delay));
    }
    anyhow::ensure!(!frames.is_empty(), "{} has no frames", path.display());
    Ok((size, frames))
}

/// A picture file as premultiplied RGBA, as the UI draws images, with its
/// size in pixels.
fn read_picture(path: &Path) -> Result<([u32; 2], Vec<u8>)> {
    let picture = image::open(path).with_context(|| format!("reading {}", path.display()))?;
    let (rgba, size) = shown_rgba(picture);
    Ok((size, rgba))
}

/// A picture as the UI draws it: no larger than [`MAX_SIDE`], as
/// premultiplied RGBA, with its size.
fn shown_rgba(picture: image::DynamicImage) -> (Vec<u8>, [u32; 2]) {
    let picture = if picture.width().max(picture.height()) > MAX_SIDE {
        picture.resize(MAX_SIDE, MAX_SIDE, FilterType::Triangle)
    } else {
        picture
    };
    let mut rgba = picture.into_rgba8();
    for pixel in rgba.pixels_mut() {
        let alpha = u16::from(pixel[3]);
        for channel in &mut pixel.0[..3] {
            *channel = (u16::from(*channel) * alpha / 255) as u8;
        }
    }
    let size = [rgba.width(), rgba.height()];
    (rgba.into_raw(), size)
}
