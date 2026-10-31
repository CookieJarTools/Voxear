//! Pictures in VoxDocs pages: each file a page shows is read, made smaller
//! if it is very large, and uploaded once for the rest of the session, a
//! couple a frame so a page full of them does not stall the editor.

use std::path::Path;

use anyhow::{Context, Result};
use image::imageops::FilterType;
use voxear_render::{Renderer, TextureSource};
use voxear_ui::ImageRef;

use super::Editor;

/// Pictures read in one frame.
const PER_FRAME: usize = 2;
/// The longest side a picture is uploaded at; a page never shows it larger.
const MAX_SIDE: u32 = 2048;

impl Editor {
    pub(super) fn update_docs_pictures(&mut self, renderer: &mut Renderer) {
        for path in self.docs.wanted_pictures(PER_FRAME) {
            let uploaded = read_picture(&path).and_then(|(size, rgba)| {
                let name = path.file_name().unwrap_or_default().to_string_lossy();
                let source = TextureSource { name: &name, width: size[0], height: size[1], rgba: &rgba, srgb: true };
                Ok((renderer.add_texture(&source)?, size))
            });
            let shown = match uploaded {
                Ok((texture, size)) => Some((ImageRef { texture: texture.0, uv: [0.0, 0.0, 1.0, 1.0] }, size)),
                Err(error) => {
                    tracing::warn!("VoxDocs: could not show {}: {error:#}", path.display());
                    None
                }
            };
            self.docs.set_picture(&path, shown);
        }
    }
}

/// A picture file as premultiplied RGBA, as the UI draws images, with its
/// size in pixels.
fn read_picture(path: &Path) -> Result<([u32; 2], Vec<u8>)> {
    let picture = image::open(path).with_context(|| format!("reading {}", path.display()))?;
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
    Ok(([rgba.width(), rgba.height()], rgba.into_raw()))
}
