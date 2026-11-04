//! Impostors for far plants: a plant drawn as one quad that turns to face
//! the camera, showing a picture of the plant from the side it is seen
//! from, lit as the plant is. Each plant's mesh and material are baked once,
//! the first time a Foliage asks, into an atlas of `IMPOSTOR_VIEWS` ×
//! `IMPOSTOR_VIEWS` pictures from all round the upper half of a sphere
//! (hemi-octahedral, as Ryan Brucks's impostors in Unreal), each holding
//! the plant's colour with its cut-out and its normals, so the impostor is
//! lit by the sun, the sky and bounced light as the plant would be.
//!
//! The shaders are `impostorBakeFragment` in `shaders/preview.slang` for the
//! bake, and `shaders/impostor.slang` for drawing.

use std::collections::{HashMap, HashSet};

use anyhow::Result;
use voxear_core::math::{Mat4, Vec3, Vec4, camera::lh::view::look_at_mat4};
use voxear_rhi::{Buffer, Gpu, ImageDesc, MemoryLocation, vk};

use crate::{
    MaterialHandle, MeshHandle,
    assets::{Assets, MaterialDesc},
    gpu::{GpuBuffers, GpuFrame, GpuInstance, Push, cols},
    graph::{FrameGraph, ImageAccess},
    mesh::{MeshData, Vertex},
    passes::{bind, set_viewport},
    pipelines::{DEPTH_FORMAT, Pipelines},
};

/// Pictures across and down an impostor's atlas, and each one's size.
pub const IMPOSTOR_VIEWS: u32 = 8;
const PICTURE: u32 = 128;
const ATLAS: u32 = IMPOSTOR_VIEWS * PICTURE;
pub const IMPOSTOR_COLOR_FORMAT: vk::Format = vk::Format::R8G8B8A8_SRGB;
pub const IMPOSTOR_NORMAL_FORMAT: vk::Format = vk::Format::R8G8B8A8_UNORM;

/// A plant's impostor: the material that draws it (its atlas's colour and
/// normals as textures), and the sphere round the plant it was pictured in,
/// in the plant's own space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Impostor {
    pub material: MaterialHandle,
    pub centre: Vec3,
    pub radius: f32,
}

/// The impostors baked, those asked for, and the quad they are drawn on.
#[derive(Default)]
pub struct ImpostorTable {
    pub ready: HashMap<(MeshHandle, MaterialHandle), Impostor>,
    /// Plants a Foliage wanted impostors of while none was baked, which the
    /// next frame bakes.
    pub asked: std::cell::RefCell<HashSet<(MeshHandle, MaterialHandle)>>,
    /// Two quads back to back, so meshlet culling never takes one for facing away.
    pub quad: Option<MeshHandle>,
}

impl ImpostorTable {
    /// A plant's impostor, or a request for one to be baked.
    pub fn get_or_ask(&self, mesh: MeshHandle, material: MaterialHandle) -> Option<Impostor> {
        let found = self.ready.get(&(mesh, material)).copied();
        if found.is_none() {
            self.asked.borrow_mut().insert((mesh, material));
        }
        found
    }
}

/// An impostor to bake this frame: where it goes, and what draws it.
pub(crate) struct Bake {
    color: (vk::Image, vk::ImageView, ImageDesc),
    normal: (vk::Image, vk::ImageView, ImageDesc),
    lod: (u32, u32),
    frames: u64,
    pointers: u64,
}

/// The buffers bakes drew from, kept until the renderer goes.
#[derive(Default)]
pub(crate) struct Bakes {
    pending: Vec<Bake>,
    buffers: Vec<Buffer>,
}

impl Bakes {
    /// Makes the atlas and material of every impostor asked for, ready to
    /// bake this frame.
    pub fn prepare(&mut self, assets: &mut Assets, gpu: &Gpu, bindless: &mut voxear_rhi::Bindless) -> Result<()> {
        let asked: Vec<_> = assets.impostors.asked.borrow_mut().drain().collect();
        if asked.is_empty() {
            return Ok(());
        }
        if assets.impostors.quad.is_none() {
            assets.impostors.quad = Some(assets.add_mesh(gpu, &facing_quad())?);
        }
        for (mesh, material) in asked {
            if assets.impostors.ready.contains_key(&(mesh, material)) {
                continue;
            }
            let (Some((centre, radius)), Some(lod)) = (assets.mesh_sphere(mesh), assets.first_lod(mesh)) else {
                continue;
            };
            let usage = vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::SAMPLED;
            let color =
                gpu.create_image("impostor colour", ImageDesc::new(ATLAS, ATLAS, IMPOSTOR_COLOR_FORMAT, usage))?;
            let normal =
                gpu.create_image("impostor normals", ImageDesc::new(ATLAS, ATLAS, IMPOSTOR_NORMAL_FORMAT, usage))?;
            let pictures = (color.handle, color.view, color.desc);
            let normals = (normal.handle, normal.view, normal.desc);
            let color_id = bindless.add_texture(gpu, color.view);
            let normal_id = bindless.add_texture(gpu, normal.view);
            assets.keep_texture(color_id, color);
            assets.keep_texture(normal_id, normal);
            let desc = MaterialDesc {
                base_color_texture: Some(color_id),
                normal_texture: Some(normal_id),
                alpha_cutoff: Some(0.5),
                foliage: assets.is_foliage(material),
                impostor: true,
                ..MaterialDesc::default()
            };
            let drawn = assets.add_material(gpu, &desc)?;
            // The plant on its own, at its own place, pictured from each way round.
            let host = MemoryLocation::CpuToGpu;
            let storage = vk::BufferUsageFlags::STORAGE_BUFFER;
            let frames = gpu.create_buffer("impostor views", (VIEWS * size_of::<GpuFrame>()) as u64, storage, host)?;
            let instance = gpu.create_buffer("impostor plant", size_of::<GpuInstance>() as u64, storage, host)?;
            let pointers = gpu.create_buffer("impostor pointers", size_of::<GpuBuffers>() as u64, storage, host)?;
            let views: Vec<GpuFrame> = (0..VIEWS).map(|view| view_frame(view as u32, centre, radius)).collect();
            frames.write(0, &views);
            let identity = cols(Mat4::IDENTITY);
            let unturned = [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0]];
            instance.write(
                0,
                &[GpuInstance {
                    model: identity,
                    prev_model: identity,
                    normal: unturned,
                    mesh: mesh.0,
                    material: material.0,
                    ..GpuInstance::default()
                }],
            );
            pointers.write(
                0,
                &[GpuBuffers {
                    instances: instance.address,
                    meshes: assets.meshes.address,
                    vertices: assets.vertices.address,
                    materials: assets.materials.address,
                    ..GpuBuffers::default()
                }],
            );
            self.pending.push(Bake {
                color: pictures,
                normal: normals,
                lod,
                frames: frames.address,
                pointers: pointers.address,
            });
            self.buffers.extend([frames, instance, pointers]);
            assets.impostors.ready.insert((mesh, material), Impostor { material: drawn, centre, radius });
        }
        Ok(())
    }

    /// Records this frame's bakes: each plant drawn once into each picture of
    /// its atlas, which shaders then sample.
    pub fn record(
        &mut self,
        fg: &mut FrameGraph<'_>,
        pipelines: &Pipelines,
        assets: &Assets,
        gpu: &Gpu,
        bindless: &mut voxear_rhi::Bindless,
    ) -> Result<()> {
        for (index, bake) in std::mem::take(&mut self.pending).into_iter().enumerate() {
            let color = fg.import_image(
                &format!("impostor colour {index}"),
                (bake.color.0, bake.color.1),
                bake.color.2,
                ImageAccess::Undefined,
            );
            let normal = fg.import_image(
                &format!("impostor normals {index}"),
                (bake.normal.0, bake.normal.1),
                bake.normal.2,
                ImageAccess::Undefined,
            );
            let depth_desc = ImageDesc::new(ATLAS, ATLAS, DEPTH_FORMAT, vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT);
            let depth = fg.create_image(gpu, bindless, "impostor depth", depth_desc)?;
            let (pipeline, indices) = (pipelines.impostor_bake, assets.indices.handle);
            let (first, count) = bake.lod;
            let images = [
                (color, ImageAccess::ColorAttachment),
                (normal, ImageAccess::ColorAttachment),
                (depth, ImageAccess::DepthAttachment),
            ];
            fg.add_pass("Impostor bake", &images, &[], move |ctx| {
                let clear = vk::ClearValue { color: vk::ClearColorValue { float32: [0.0; 4] } };
                let target = |view| {
                    vk::RenderingAttachmentInfo::default()
                        .image_view(view)
                        .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                        .load_op(vk::AttachmentLoadOp::CLEAR)
                        .clear_value(clear)
                        .store_op(vk::AttachmentStoreOp::STORE)
                };
                let targets = [target(ctx.view(color)), target(ctx.view(normal))];
                let far = vk::ClearValue { depth_stencil: vk::ClearDepthStencilValue { depth: 0.0, stencil: 0 } };
                let depth = vk::RenderingAttachmentInfo::default()
                    .image_view(ctx.view(depth))
                    .image_layout(vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL)
                    .load_op(vk::AttachmentLoadOp::CLEAR)
                    .clear_value(far)
                    .store_op(vk::AttachmentStoreOp::DONT_CARE);
                let area = vk::Rect2D {
                    offset: vk::Offset2D::default(),
                    extent: vk::Extent2D { width: ATLAS, height: ATLAS },
                };
                let info = vk::RenderingInfo::default()
                    .render_area(area)
                    .layer_count(1)
                    .color_attachments(&targets)
                    .depth_attachment(&depth);
                unsafe {
                    ctx.gpu.device.cmd_begin_rendering(ctx.cmd, &info);
                    ctx.gpu.device.cmd_bind_index_buffer(ctx.cmd, indices, 0, vk::IndexType::UINT32);
                }
                for view in 0..VIEWS as u32 {
                    let at = vk::Offset2D {
                        x: ((view % IMPOSTOR_VIEWS) * PICTURE) as i32,
                        y: ((view / IMPOSTOR_VIEWS) * PICTURE) as i32,
                    };
                    set_viewport(ctx, at, vk::Extent2D { width: PICTURE, height: PICTURE });
                    let frame = bake.frames + u64::from(view) * size_of::<GpuFrame>() as u64;
                    bind(
                        ctx,
                        pipeline,
                        vk::PipelineBindPoint::GRAPHICS,
                        &Push { frame, buffers: bake.pointers, list: 0, ..Push::default() },
                    );
                    unsafe { ctx.gpu.device.cmd_draw_indexed(ctx.cmd, count, 1, first, 0, 0) };
                }
                unsafe { ctx.gpu.device.cmd_end_rendering(ctx.cmd) };
            });
            fg.transition(color, ImageAccess::Sampled);
            fg.transition(normal, ImageAccess::Sampled);
        }
        Ok(())
    }

    pub fn take_buffers(&mut self) -> Vec<Buffer> {
        std::mem::take(&mut self.buffers)
    }
}

const VIEWS: usize = (IMPOSTOR_VIEWS * IMPOSTOR_VIEWS) as usize;

/// The direction a picture of the atlas looks at the plant from, from the
/// plant: its place in the grid, hemi-octahedrally, as
/// `impostorDirection` in `shaders/impostor.slang` reads it.
pub fn picture_direction(view: u32) -> Vec3 {
    let (x, y) = (view % IMPOSTOR_VIEWS, view / IMPOSTOR_VIEWS);
    let grid = |i: u32| (i as f32 + 0.5) / IMPOSTOR_VIEWS as f32 * 2.0 - 1.0;
    let (gx, gy) = (grid(x), grid(y));
    let (px, pz) = ((gx + gy) * 0.5, (gx - gy) * 0.5);
    Vec3::new(px, 1.0 - px.abs() - pz.abs(), pz).normalize()
}

/// The camera for one picture: looking at the sphere round the plant from
/// `picture_direction`, square on, just wide enough to take it in.
fn view_frame(view: u32, centre: Vec3, radius: f32) -> GpuFrame {
    let toward = picture_direction(view);
    let eye = centre + toward * radius * 2.0;
    let up = if toward.y.abs() > 0.99 { Vec3::Z } else { Vec3::Y };
    let look = look_at_mat4(eye, centre, up);
    // Square on, reverse Z: the near side of the sphere at 1, the far at 0. Y flips as the perspective cameras do.
    let (near, far) = (radius * 0.9, radius * 3.1);
    let flip =
        voxear_core::math::camera::lh::proj::vulkan::perspective_infinite_reverse(1.0, 1.0, 0.1).y_axis.y.signum();
    let ortho = Mat4::from_cols(
        Vec4::new(1.0 / radius, 0.0, 0.0, 0.0),
        Vec4::new(0.0, flip / radius, 0.0, 0.0),
        Vec4::new(0.0, 0.0, -1.0 / (far - near), 0.0),
        Vec4::new(0.0, 0.0, far / (far - near), 1.0),
    );
    GpuFrame {
        view_proj: cols(ortho * look),
        camera_position: eye.to_array(),
        camera_forward: (-toward).to_array(),
        ..GpuFrame::default()
    }
}

/// A square two units across in X and Y, facing +Z and again facing -Z.
fn facing_quad() -> MeshData {
    let corner = |x: f32, y: f32, z: f32| Vertex {
        position: [x, y, 0.0],
        normal: [0.0, 0.0, z],
        uv: [x * 0.5 + 0.5, 0.5 - y * 0.5],
        tangent: [z, 0.0, 0.0, 1.0],
        wind: crate::mesh::NO_WIND_MASKS,
    };
    let mut mesh = MeshData::default();
    for z in [1.0, -1.0] {
        mesh.vertices.extend([corner(-1.0, -1.0, z), corner(1.0, -1.0, z), corner(1.0, 1.0, z), corner(-1.0, 1.0, z)]);
    }
    mesh.indices = vec![0, 1, 2, 0, 2, 3, 4, 6, 5, 4, 7, 6];
    mesh
}
