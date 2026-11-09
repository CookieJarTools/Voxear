//! UI panels in the world: Voxear UI documents shown on rectangles in 3D,
//! which players read and click (see [`UiPanel`]).
//!
//! The engine lays each panel's UI out on a `Ui` of its own and hands the
//! renderer its quads ([`UiPanelFrame`]), drawn at the size the panel last
//! showed on screen. Before the views, each panel whose UI changed is drawn
//! into a texture of its own with the UI's own shader, and its mips are made
//! from it. Each view then draws its panels over its finished picture, after
//! post-processing, so their colours are as designed and TAA does not soften
//! their text, and hides them where the scene is in front. While it draws
//! them, it works out how large each shows, which the engine reads back
//! ([`crate::Renderer::ui_panel_sizes`]) to size the next drawing.
//!
//! The shaders are `uiVertex` and `uiFragment` for a panel's texture, and
//! `uiPanelVertex`, `uiPanelFragment` and `uiPanelMipMain`, in
//! `shaders/ui.slang`.

use std::collections::HashMap;

use anyhow::Result;
use voxear_assets::AssetGuid;
use voxear_core::math::{Mat4, Vec3, Vec4};
use voxear_ecs::{Component, Entity, World};
use voxear_rhi::{Bindless, Buffer, Gpu, ImageDesc, TextureId, vk};
use voxear_scene::{Destroying, Transform, WorldTransform};
use voxear_ui::{Quad, UiFrame};

use crate::{
    gpu::{GpuUiPanel, NO_TEXTURE, Push},
    graph::{FrameGraph, ImageAccess, ImageId, RenderGraph},
    passes::{begin_rendering_in, bind},
    ui_pass::{Layer, MAX_UI_QUADS, MAX_UI_VALUES},
};

/// The most UI panels a view draws, the nearest first.
pub const MAX_UI_PANELS: usize = 64;
/// What a panel's UI is drawn into: linear, so it blends as the screen's UI
/// does, and writable by the compute shader that makes its mips.
pub(crate) const PANEL_FORMAT: vk::Format = vk::Format::R16G16B16A16_SFLOAT;

/// A UI document shown on a rectangle in the world, centred on its Obj. It
/// reads the right way round looking the way its Obj faces, along its
/// forward; its right and up are its Obj's. Two-sided, it reads the right
/// way round from behind too, as a sign painted on both sides does.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiPanel {
    /// The `.vui` document it shows.
    pub document: Option<AssetGuid>,
    /// How wide and high it is, in metres, before its Obj's scale.
    pub size: [f32; 2],
    /// How many of the UI's pixels a metre holds, so a panel 1.6 m wide at
    /// 500 lays its UI out 800 pixels wide.
    pub pixels_per_metre: f32,
    /// Turned to face the camera, keeping its up, rather than its Obj's way.
    pub face_camera: bool,
    /// Shown, and worked, from behind as well as from the front.
    pub two_sided: bool,
    /// Whether the pointer works it. Off, clicks pass through to the game.
    pub clickable: bool,
}

impl Component for UiPanel {}

impl Default for UiPanel {
    fn default() -> Self {
        Self {
            document: None,
            size: [1.6, 0.9],
            pixels_per_metre: 500.0,
            face_camera: false,
            two_sided: true,
            clickable: true,
        }
    }
}

impl UiPanel {
    /// The size its UI is laid out at, in the UI's pixels.
    pub fn layout_size(&self) -> [f32; 2] {
        let ppm = self.pixels_per_metre.max(1.0);
        [(self.size[0].max(0.01) * ppm).max(1.0), (self.size[1].max(0.01) * ppm).max(1.0)]
    }
}

/// Where a panel is in the world: its top left corner, and the edges from
/// there to its top right and its bottom left.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PanelRect {
    pub origin: Vec3,
    pub across: Vec3,
    pub down: Vec3,
}

impl PanelRect {
    /// Where `panel` is on its Obj placed by `world`, turned to face a
    /// camera at `eye` when it asks to.
    pub fn of(panel: &UiPanel, world: Mat4, eye: Vec3) -> Self {
        let centre = world.w_axis.truncate();
        let (right, up) = (world.x_axis.truncate(), world.y_axis.truncate());
        let (right, up) = if panel.face_camera {
            let forward = (centre - eye).normalize_or(world.z_axis.truncate().normalize_or(Vec3::Z));
            let turned = Vec3::Y.cross(forward).normalize_or(Vec3::X);
            (turned * right.length(), forward.cross(turned) * up.length())
        } else {
            (right, up)
        };
        let across = right * panel.size[0].max(0.0);
        let down = -up * panel.size[1].max(0.0);
        Self { origin: centre - across * 0.5 - down * 0.5, across, down }
    }

    /// The same rectangle read from its back, the right way round there:
    /// its left and right change places.
    pub fn turned(&self) -> Self {
        Self { origin: self.origin + self.across, across: -self.across, down: self.down }
    }

    /// Its middle.
    pub fn centre(&self) -> Vec3 {
        self.origin + (self.across + self.down) * 0.5
    }

    /// The way it is read: from its front, into it.
    pub fn forward(&self) -> Vec3 {
        self.across.cross(-self.down).normalize_or_zero()
    }

    /// Whether `eye` sees its front.
    pub fn faces(&self, eye: Vec3) -> bool {
        (self.centre() - eye).dot(self.forward()) > 0.0
    }

    /// Where a ray from `from` along `direction` meets its front: how far
    /// along, in lengths of `direction`, and where on it, from 0 to 1 across
    /// and down.
    pub fn hit(&self, from: Vec3, direction: Vec3) -> Option<(f32, [f32; 2])> {
        let normal = self.forward();
        let towards = direction.dot(normal);
        if towards <= 1e-6 {
            return None;
        }
        let along = (self.origin - from).dot(normal) / towards;
        if along < 0.0 {
            return None;
        }
        let on = from + direction * along - self.origin;
        let (width, height) = (self.across.length_squared(), self.down.length_squared());
        if width <= 0.0 || height <= 0.0 {
            return None;
        }
        let uv = [on.dot(self.across) / width, on.dot(self.down) / height];
        uv.iter().all(|t| (0.0..=1.0).contains(t)).then_some((along, uv))
    }

    /// How many pixels wide and high it shows through `view_projection` on
    /// a view `viewport` pixels in size, along its longer edges. A corner
    /// behind the camera means it is nearly at the eye, as big as it gets.
    pub fn on_screen(&self, view_projection: Mat4, viewport: [f32; 2]) -> [f32; 2] {
        let corners = [self.origin, self.origin + self.across, self.origin + self.down, self.origin + self.across + self.down];
        let mut pixels = [[0.0f32; 2]; 4];
        for (pixel, corner) in pixels.iter_mut().zip(corners) {
            let clip = view_projection * Vec4::from((corner, 1.0));
            if clip.w <= 1e-4 {
                return [f32::MAX; 2];
            }
            *pixel = [clip.x / clip.w * viewport[0] * 0.5, clip.y / clip.w * viewport[1] * 0.5];
        }
        let length = |a: [f32; 2], b: [f32; 2]| ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt();
        let width = length(pixels[0], pixels[1]).max(length(pixels[2], pixels[3]));
        let height = length(pixels[0], pixels[2]).max(length(pixels[1], pixels[3]));
        [width, height]
    }
}

/// The world matrix of an Obj, from its `WorldTransform` or, before one is
/// worked out, its own `Transform`.
fn placed(transform: &Transform, world: Option<&WorldTransform>) -> Mat4 {
    world.map_or_else(
        || Mat4::from_scale_rotation_translation(transform.scale, transform.rotation, transform.position),
        |world| world.0,
    )
}

/// The panels in `world`, each by its Obj, where it is for a camera at
/// `eye`, and its settings. A two-sided panel seen from behind is given as
/// read from there, turned (see [`PanelRect::turned`]).
pub fn panels(world: &World, eye: Vec3) -> Vec<(Entity, PanelRect, UiPanel)> {
    let query = world.query_ref::<(Entity, &UiPanel, &Transform, Option<&WorldTransform>)>();
    let seen = |panel: &UiPanel, rect: PanelRect| match panel.two_sided && !rect.faces(eye) {
        true => rect.turned(),
        false => rect,
    };
    query
        .without::<Destroying>()
        .into_iter()
        .map(|(entity, panel, transform, at)| (entity, seen(panel, PanelRect::of(panel, placed(transform, at), eye)), *panel))
        .collect()
}

/// A panel's UI, laid out by the engine, for its texture: `size` is the
/// texture's size in pixels, which the frame's quads are laid out in.
pub struct UiPanelFrame<'a> {
    pub entity: Entity,
    pub frame: &'a UiFrame,
    pub size: [u32; 2],
}

/// One panel's UI on the GPU: its quads and glyph atlas, the size of its
/// texture, and whether its texture needs drawing again.
#[derive(Default)]
struct PanelGpu {
    layer: Layer,
    size: [u32; 2],
    stale: bool,
}

/// This frame's texture for a panel, for the views to draw it with.
#[derive(Clone, Copy)]
pub(crate) struct PanelTexture {
    pub entity: Entity,
    pub image: ImageId,
    pub texture: TextureId,
}

/// Every panel's UI, kept until the engine hands over the next, and how
/// large each showed in this frame's views and the last's.
#[derive(Default)]
pub(crate) struct UiPanels {
    panels: HashMap<Entity, PanelGpu>,
    seen: HashMap<Entity, [f32; 2]>,
    shown: HashMap<Entity, [f32; 2]>,
}

/// The name a panel's texture is kept by in the frame graph.
fn image_name(entity: Entity) -> String {
    format!("ui panel {:x}", entity.to_bits())
}

impl UiPanels {
    /// Takes this frame's panels. Those not among them are let go.
    pub fn set(
        &mut self,
        gpu: &Gpu,
        bindless: &mut Bindless,
        graph: &mut RenderGraph,
        frames: &[UiPanelFrame<'_>],
    ) -> Result<()> {
        let gone: Vec<Entity> =
            self.panels.keys().filter(|&&entity| !frames.iter().any(|f| f.entity == entity)).copied().collect();
        if !gone.is_empty() {
            gpu.wait_idle();
            for entity in gone {
                if let Some(mut panel) = self.panels.remove(&entity) {
                    panel.layer.destroy(gpu, bindless);
                }
                graph.release(gpu, bindless, &image_name(entity));
            }
        }
        for frame in frames {
            let panel = self.panels.entry(frame.entity).or_default();
            let atlas = frame.frame.atlas.as_ref().map(|atlas| atlas.version);
            // A shaded background may change every frame with the time, so its panel is drawn every frame.
            let shaded = frame.frame.quads.iter().any(|quad| quad.flags & Quad::SHADED != 0);
            let changed = shaded
                || panel.size != frame.size
                || panel.layer.quads != frame.frame.quads
                || atlas.is_some_and(|version| version != panel.layer.version);
            panel.stale |= changed;
            panel.size = frame.size;
            panel.layer.set(gpu, bindless, frame.frame, MAX_UI_QUADS as usize / 2)?;
        }
        Ok(())
    }

    /// The largest each panel showed in the last frame's views, in pixels.
    pub fn sizes(&self) -> &HashMap<Entity, [f32; 2]> {
        &self.shown
    }

    /// Notes how large a panel showed in a view this frame.
    pub fn saw(&mut self, entity: Entity, size: [f32; 2]) {
        let seen = self.seen.entry(entity).or_insert([0.0; 2]);
        *seen = [seen[0].max(size[0]), seen[1].max(size[1])];
    }

    /// Ends the frame: what the views saw becomes what was shown.
    pub fn end_frame(&mut self) {
        self.shown = std::mem::take(&mut self.seen);
    }

    /// Draws each panel whose UI changed into its texture, with its mips,
    /// its quads from quad `first` of `buffer` on and its shaded
    /// backgrounds' values from `first_value` of `value_buffer` on. Returns
    /// the panels' textures, and how many quads and values it wrote.
    pub fn record(
        &mut self,
        fg: &mut FrameGraph<'_>,
        (gpu, bindless): (&Gpu, &mut Bindless),
        (draw, mips, push): (vk::Pipeline, vk::Pipeline, Push),
        (buffer, value_buffer): (&Buffer, &Buffer),
        (first, first_value): (usize, usize),
    ) -> Result<(Vec<PanelTexture>, (usize, usize))> {
        let mut textures = Vec::with_capacity(self.panels.len());
        let (mut written, mut values_written) = (0, 0);
        for (&entity, panel) in &mut self.panels {
            let [width, height] = panel.size;
            if width == 0 || height == 0 {
                continue;
            }
            let usage =
                vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::SAMPLED;
            let desc = ImageDesc::new(width, height, PANEL_FORMAT, usage).with_mips();
            let image = fg.history_image(gpu, bindless, &image_name(entity), desc)?;
            let texture = fg.texture(image);
            textures.push(PanelTexture { entity, image, texture });
            let room = (MAX_UI_QUADS as usize).saturating_sub(first + written);
            if !(panel.stale || fg.is_new(image)) || room == 0 {
                continue;
            }
            panel.stale = false;
            let quads = panel.layer.quads.len().min(room);
            let at = first + written;
            buffer.write((at * size_of::<Quad>()) as u64, &panel.layer.quads[..quads]);
            written += quads;
            let value_at = first_value + values_written;
            let values = panel.layer.values.len().min((MAX_UI_VALUES as usize).saturating_sub(value_at));
            value_buffer.write((value_at * size_of::<[f32; 4]>()) as u64, &panel.layer.values[..values]);
            values_written += values;
            let base = Push { size: [width, height], textures: [NO_TEXTURE, 0, 0, 0], ..push };
            let drawn =
                Push { texture: panel.layer.texture(), list: 0, level: at as u32, max_draws: value_at as u32, ..base };
            let area = vk::Rect2D { offset: vk::Offset2D::default(), extent: desc.extent() };
            fg.add_pass("UI panel", &[(image, ImageAccess::ColorAttachment)], &[], move |ctx| {
                let color = [vk::RenderingAttachmentInfo::default()
                    .image_view(ctx.first_mip(image))
                    .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                    .load_op(vk::AttachmentLoadOp::CLEAR)
                    .clear_value(vk::ClearValue { color: vk::ClearColorValue { float32: [0.0; 4] } })
                    .store_op(vk::AttachmentStoreOp::STORE)];
                begin_rendering_in(ctx, area, &color);
                if quads > 0 {
                    bind(ctx, draw, vk::PipelineBindPoint::GRAPHICS, &drawn);
                    unsafe { ctx.gpu.device.cmd_draw(ctx.cmd, quads as u32 * 6, 1, 0, 0) };
                }
                unsafe { ctx.gpu.device.cmd_end_rendering(ctx.cmd) };
            });
            let outputs: Vec<_> = (1..desc.mip_levels as usize).map(|mip| fg.storage(image, mip)).collect();
            fg.add_pass("UI panel mips", &[(image, ImageAccess::StorageWrite)], &[], move |ctx| {
                for (index, output) in outputs.iter().enumerate() {
                    let mip = index as u32 + 1;
                    let size = [(width >> mip).max(1), (height >> mip).max(1)];
                    let push = Push { texture: texture.0, level: mip - 1, output: output.0, size, ..base };
                    bind(ctx, mips, vk::PipelineBindPoint::COMPUTE, &push);
                    unsafe { ctx.gpu.device.cmd_dispatch(ctx.cmd, size[0].div_ceil(8), size[1].div_ceil(8), 1) };
                    // Each mip reads the one before.
                    let barrier = [vk::MemoryBarrier2::default()
                        .src_stage_mask(vk::PipelineStageFlags2::COMPUTE_SHADER)
                        .src_access_mask(vk::AccessFlags2::SHADER_STORAGE_WRITE)
                        .dst_stage_mask(vk::PipelineStageFlags2::COMPUTE_SHADER)
                        .dst_access_mask(vk::AccessFlags2::SHADER_SAMPLED_READ)];
                    let dependency = vk::DependencyInfo::default().memory_barriers(&barrier);
                    unsafe { ctx.gpu.device.cmd_pipeline_barrier2(ctx.cmd, &dependency) };
                }
            });
        }
        Ok((textures, (written, values_written)))
    }

    /// The caller makes sure the GPU is idle.
    pub fn destroy(&mut self, gpu: &Gpu, bindless: &mut Bindless) {
        for (_, mut panel) in self.panels.drain() {
            panel.layer.destroy(gpu, bindless);
        }
    }
}

/// What a view needs to draw its panels.
pub(crate) struct PanelView<'a> {
    /// This frame's panels' textures.
    pub textures: &'a [PanelTexture],
    pub camera: Vec3,
    pub view_projection: Mat4,
    /// Where the view is in the output, and the size its scene was drawn at,
    /// which its depth has.
    pub view: vk::Rect2D,
    pub render: vk::Extent2D,
}

/// Writes the panels `world` holds that have a texture, the farthest first,
/// into `buffer`, and adds the pass that draws them onto `output` over the
/// view's finished picture, hidden where `depth` is in front. Returns how
/// large each showed, in pixels.
pub(crate) fn draw(
    fg: &mut FrameGraph<'_>,
    (pipeline, push): (vk::Pipeline, Push),
    buffer: &Buffer,
    world: &World,
    panels: &PanelView<'_>,
    (output, depth): (ImageId, ImageId),
) -> Vec<(Entity, [f32; 2])> {
    if panels.textures.is_empty() {
        return Vec::new();
    }
    let viewport = [panels.view.extent.width as f32, panels.view.extent.height as f32];
    let mut shown: Vec<(f32, Entity, PanelRect, PanelTexture)> = self::panels(world, panels.camera)
        .into_iter()
        .filter(|(_, rect, _)| rect.faces(panels.camera))
        .filter_map(|(entity, rect, _)| {
            let texture = panels.textures.iter().find(|t| t.entity == entity)?;
            Some((rect.centre().distance_squared(panels.camera), entity, rect, *texture))
        })
        .collect();
    shown.sort_by(|a, b| a.0.total_cmp(&b.0));
    shown.truncate(MAX_UI_PANELS);
    shown.reverse();
    let sizes = shown.iter().map(|(_, entity, rect, _)| (*entity, rect.on_screen(panels.view_projection, viewport))).collect();
    if shown.is_empty() {
        return sizes;
    }
    let gpu: Vec<GpuUiPanel> = shown
        .iter()
        .map(|(_, _, rect, texture)| GpuUiPanel {
            origin: rect.origin.to_array(),
            texture: texture.texture.0,
            across: rect.across.to_array(),
            down: rect.down.to_array(),
            ..Default::default()
        })
        .collect();
    buffer.write(0, &gpu);
    let pack = |a: u32, b: u32| a.min(0xffff) | b.min(0xffff) << 16;
    let view = panels.view;
    let offset = [view.offset.x as u32, view.offset.y as u32];
    let push = Push {
        textures: [fg.texture(depth).0, offset[0], offset[1], voxear_rhi::samplers::ANISOTROPIC_CLAMP],
        size: [panels.render.width, panels.render.height],
        output: pack(view.extent.width, view.extent.height),
        ..push
    };
    let mut images = vec![(output, ImageAccess::ColorAttachment), (depth, ImageAccess::Sampled)];
    images.extend(shown.iter().map(|(.., texture)| (texture.image, ImageAccess::Sampled)));
    let vertices = gpu.len() as u32 * 6;
    fg.add_pass("UI panels", &images, &[], move |ctx| {
        let color = [vk::RenderingAttachmentInfo::default()
            .image_view(ctx.view(output))
            .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .load_op(vk::AttachmentLoadOp::LOAD)
            .store_op(vk::AttachmentStoreOp::STORE)];
        begin_rendering_in(ctx, view, &color);
        bind(ctx, pipeline, vk::PipelineBindPoint::GRAPHICS, &push);
        unsafe { ctx.gpu.device.cmd_draw(ctx.cmd, vertices, 1, 0, 0) };
        unsafe { ctx.gpu.device.cmd_end_rendering(ctx.cmd) };
    });
    sizes
}

#[cfg(test)]
mod tests {
    use voxear_core::math::Quat;

    use super::*;

    fn at(position: Vec3, rotation: Quat) -> Mat4 {
        Mat4::from_rotation_translation(rotation, position)
    }

    #[test]
    fn a_panel_reads_looking_the_way_its_obj_faces() {
        let panel = UiPanel { size: [2.0, 1.0], ..UiPanel::default() };
        let rect = PanelRect::of(&panel, at(Vec3::new(0.0, 1.0, 5.0), Quat::IDENTITY), Vec3::ZERO);
        assert_eq!(rect.origin, Vec3::new(-1.0, 1.5, 5.0), "its top left");
        assert_eq!(rect.across, Vec3::new(2.0, 0.0, 0.0));
        assert_eq!(rect.down, Vec3::new(0.0, -1.0, 0.0));
        assert_eq!(rect.forward(), Vec3::Z);
        assert!(rect.faces(Vec3::ZERO), "seen from behind its Obj");
        assert!(!rect.faces(Vec3::new(0.0, 1.0, 10.0)), "not from in front of it");
    }

    #[test]
    fn a_ray_finds_where_on_the_panel_it_lands() {
        let panel = UiPanel { size: [2.0, 1.0], ..UiPanel::default() };
        let rect = PanelRect::of(&panel, at(Vec3::new(0.0, 1.0, 5.0), Quat::IDENTITY), Vec3::ZERO);
        let (along, uv) = rect.hit(Vec3::new(0.0, 1.0, 0.0), Vec3::Z).expect("straight at its middle");
        assert!((along - 5.0).abs() < 1e-5);
        assert!((uv[0] - 0.5).abs() < 1e-5 && (uv[1] - 0.5).abs() < 1e-5);
        let (_, corner) = rect.hit(Vec3::new(-0.9, 1.4, 0.0), Vec3::Z).expect("near its top left");
        assert!((corner[0] - 0.05).abs() < 1e-5 && (corner[1] - 0.1).abs() < 1e-5, "{corner:?}");
        assert!(rect.hit(Vec3::new(1.5, 1.0, 0.0), Vec3::Z).is_none(), "past its right edge");
        assert!(rect.hit(Vec3::new(0.0, 1.0, 10.0), -Vec3::Z).is_none(), "from behind");
        assert!(rect.hit(Vec3::new(0.0, 1.0, 0.0), -Vec3::Z).is_none(), "pointing away");
    }

    #[test]
    fn a_two_sided_panel_reads_the_right_way_round_from_behind() {
        let mut world = World::new();
        let one_sided = UiPanel { size: [2.0, 1.0], two_sided: false, ..UiPanel::default() };
        let placed = Transform::from_position(Vec3::new(0.0, 0.0, 5.0));
        let front = world.spawn((placed, one_sided));
        let both = world.spawn((placed, UiPanel { two_sided: true, ..one_sided }));
        let behind = Vec3::new(0.0, 0.0, 10.0);
        let rects = panels(&world, behind);
        let rect = |entity: Entity| rects.iter().find(|(e, ..)| *e == entity).unwrap().1;
        assert!(!rect(front).faces(behind), "one-sided, its back is all there is from here");
        let back = rect(both);
        assert!(back.faces(behind), "two-sided, it faces whoever looks");
        // Looking along -Z, the viewer's right is -X: the panel's left edge in the front's terms.
        assert_eq!(back.origin, Vec3::new(1.0, 0.5, 5.0));
        assert_eq!(back.across, Vec3::new(-2.0, 0.0, 0.0));
        let (_, [u, v]) = back.hit(behind, Vec3::new(-0.5, 0.0, -5.0).normalize()).expect("from behind, on its right");
        assert!((u - 0.75).abs() < 1e-5 && (v - 0.5).abs() < 1e-5, "the right of the page as read from here: {u} {v}");
    }

    #[test]
    fn facing_the_camera_turns_it_but_keeps_it_upright() {
        let panel = UiPanel { size: [1.0, 1.0], face_camera: true, ..UiPanel::default() };
        let placed = at(Vec3::new(3.0, 1.0, 0.0), Quat::from_rotation_y(1.0));
        let eye = Vec3::new(0.0, 1.0, 0.0);
        let rect = PanelRect::of(&panel, placed, eye);
        assert!((rect.forward() - Vec3::X).length() < 1e-5, "read looking from the camera to it");
        assert!(rect.down.x.abs() < 1e-5 && rect.down.z.abs() < 1e-5 && rect.down.y < 0.0, "upright");
        assert!(rect.hit(eye, Vec3::X).is_some());
    }

    #[test]
    fn its_size_on_screen_shrinks_with_distance() {
        let panel = UiPanel { size: [1.0, 1.0], ..UiPanel::default() };
        let projection = voxear_core::math::camera::lh::proj::vulkan::perspective_infinite_reverse(1.0, 1.0, 0.1);
        let near = PanelRect::of(&panel, at(Vec3::new(0.0, 0.0, 2.0), Quat::IDENTITY), Vec3::ZERO);
        let far = PanelRect::of(&panel, at(Vec3::new(0.0, 0.0, 8.0), Quat::IDENTITY), Vec3::ZERO);
        let [near_width, _] = near.on_screen(projection, [800.0, 800.0]);
        let [far_width, _] = far.on_screen(projection, [800.0, 800.0]);
        assert!((near_width / far_width - 4.0).abs() < 1e-3, "{near_width} {far_width}");
        let behind = PanelRect::of(&panel, at(Vec3::new(0.0, 0.0, -2.0), Quat::IDENTITY), Vec3::ZERO);
        assert_eq!(behind.on_screen(projection, [800.0, 800.0]), [f32::MAX; 2]);
    }
}
