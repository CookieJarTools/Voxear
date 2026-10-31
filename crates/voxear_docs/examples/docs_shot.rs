//! Renders the VoxDocs panel for a project's `Docs` to a PNG, without the
//! editor: `cargo run -p voxear_docs --example docs_shot -- <project> <page> <out.png>`.

use std::path::PathBuf;

use anyhow::{Context, Result};
use voxear_docs::VoxDocs;
use voxear_ecs::World;
use voxear_render::{Camera, Renderer, RendererSettings};
use voxear_scene::Transform;
use voxear_ui::{Style, Ui};

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let project = PathBuf::from(args.next().context("a project folder")?);
    let page = args.next().unwrap_or_default();
    let out = PathBuf::from(args.next().unwrap_or_else(|| "docs.png".into()));
    let (width, height) = (1100, 760);
    let mut renderer = Renderer::new_offscreen(width, height, &RendererSettings::default())?;
    renderer.set_clear_color([0.004, 0.004, 0.005, 1.0]);
    let mut world = World::new();
    world.spawn((Camera::default(), Transform::IDENTITY));

    let mut ui = Ui::new();
    let root = ui.root();
    let host = ui.panel(root, Style::column().at(8.0, 8.0).width(width as f32 - 16.0).height(height as f32 - 16.0));
    ui.set_inline_style(
        host,
        "border-radius: 8px; background-color: #191a1c; border: 1px solid #25272b; overflow: hidden",
    );
    let mut docs = VoxDocs::new(&mut ui, host, &project, "Hello");
    if !page.is_empty() {
        docs.open_reference(&mut ui, &page);
    }
    for _ in 0..3 {
        ui.advance(0.5);
        docs.update(&mut ui);
        let frame = ui.frame(width, height, 1.0);
        renderer.set_ui(&frame)?;
        renderer.render(&world, 0.0)?;
    }
    for warning in ui.take_warnings() {
        eprintln!("warning: {warning}");
    }
    let (w, h, pixels) = renderer.read_pixels()?;
    let file = std::fs::File::create(&out)?;
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), w, h);
    encoder.set_color(png::ColorType::Rgba);
    encoder.write_header()?.write_image_data(&pixels)?;
    println!("wrote {}", out.display());
    Ok(())
}
