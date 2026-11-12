//! Maps of a world of blocks, drawn as a small model of it: a slab of the
//! ground round a point, with its hills, cliffs, water and whatever stands
//! on it, seen from a camera turned and tilted over it, for a minimap or a
//! map opened over the game.
//!
//! A map keeps a window of columns (see `voxear_terrain::blocks::map`), as
//! many cells a side as it was made with, each kept at its place modulo the
//! window's side: so as the window moves with what it shows, only the tiles
//! that came into it are written. Nothing is meshed. The shader,
//! `shaders/map.slang`, walks each pixel's ray through the columns.
//!
//! A map is drawn into an image of its own before the UI, which shows it as
//! any picture (see [`MapImage`]), and only when what it shows or how it is
//! seen changed: a map left alone costs nothing.

use std::collections::HashMap;

use anyhow::Result;
use voxear_core::math::{Vec2, Vec3};
use voxear_rhi::{Bindless, Buffer, Gpu, ImageDesc, MemoryLocation, TextureId, vk};
use voxear_terrain::blocks::map::{MAP_TILE, MapColumn, MapPiece};

use crate::{
    gpu::Push,
    graph::{BufferAccess, FrameGraph, ImageAccess, ImageId},
    passes::{attachment, begin_rendering, bind},
    pipelines::{HDR_FORMAT, Pipelines},
};

/// The largest a map is drawn, in pixels a side.
pub const MAX_MAP_SIZE: u32 = 4096;

/// Maps drawn in one frame at most.
const MAX_VIEWS: usize = 16;

/// The cells along a side of each group whose highest block is kept beside
/// the columns, for rays to step over a group none of whose columns reaches
/// up to them: `SKIPS` in `shaders/map.slang`, as powers of two. A small
/// group, and a large one, which is a tile.
const GROUP: usize = 8;
const LARGE_GROUP: usize = MAP_TILE;

/// A map made with [`Renderer::add_map`](crate::Renderer::add_map).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MapId(pub u32);

/// One column of a map as its shader reads it; mirrors `MapColumn` in
/// `shaders/map.slang`.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuMapColumn {
    top: i32,
    water: i32,
    highest: i32,
    layers: u32,
    deep: u32,
    depths: u32,
    piece0: u32,
    piece0_blocks: u32,
    piece1: u32,
    piece1_blocks: u32,
}

impl GpuMapColumn {
    /// A column with nothing in it.
    pub const NONE: GpuMapColumn = GpuMapColumn {
        top: i32::MIN,
        water: i32::MIN,
        highest: i32::MIN,
        layers: 0,
        deep: 0,
        depths: 0,
        piece0: 0,
        piece0_blocks: 0,
        piece1: 0,
        piece1_blocks: 0,
    };

    pub fn of(column: &MapColumn) -> Self {
        if !column.is_some() {
            return Self::NONE;
        }
        let pair = |low: u16, high: u16| u32::from(low) | u32::from(high) << 16;
        let over = |height: i32| (height - column.top).clamp(0, 0xffff) as u16;
        let span = |piece: &MapPiece| if piece.is_some() { pair(over(piece.from), over(piece.to)) } else { 0 };
        let [first, second] = &column.pieces;
        GpuMapColumn {
            top: column.top,
            water: column.water.max(column.top),
            highest: column.highest(),
            layers: pair(column.blocks[0], column.blocks[1]),
            deep: pair(column.blocks[2], column.water_block),
            depths: u32::from(column.depths[0]) | u32::from(column.depths[1]) << 8,
            piece0: span(first),
            piece0_blocks: pair(first.top, first.side),
            piece1: span(second),
            piece1_blocks: pair(second.top, second.side),
        }
    }
}

/// What a map is drawn as; mirrors `MapView` in `shaders/map.slang`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
struct GpuMapView {
    origin: [f32; 3],
    reach: f32,
    forward: [f32; 3],
    bottom: f32,
    right: [f32; 3],
    ceiling: f32,
    up: [f32; 3],
    samples: u32,
    middle: [f32; 2],
    size: [f32; 2],
    corner: [i32; 2],
    base: i32,
    side: u32,
    sun: [f32; 3],
    shadows: f32,
    round: u32,
    blocks: u32,
    edges: f32,
    lowest: f32,
}

/// Where a map is seen from: looking at `middle`, in cells, from `distance`
/// cells off, turned `turn` radians from north (0 looks north, east to the
/// right) and tilted `tilt` radians over the ground (a quarter turn looks
/// straight down), `fov` radians from the view's bottom to its top.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MapCamera {
    pub middle: Vec3,
    pub turn: f32,
    pub tilt: f32,
    pub distance: f32,
    pub fov: f32,
}

/// The least a map's camera tilts over the ground: lower and its rays would
/// no longer all go down, which the shader counts on.
pub const MAP_LEAST_TILT: f32 = 0.35;

impl MapCamera {
    fn tilt(&self) -> f32 {
        self.tilt.clamp(MAP_LEAST_TILT, std::f32::consts::FRAC_PI_2)
    }

    fn fov(&self) -> f32 {
        self.fov.clamp(0.05, 0.6)
    }

    /// The way it looks, to its right and up its view.
    pub fn axes(&self) -> [Vec3; 3] {
        let (sin, cos) = self.turn.sin_cos();
        let (down, along) = self.tilt().sin_cos();
        let forward = Vec3::new(sin * along, -down, cos * along);
        let right = Vec3::new(cos, 0.0, -sin);
        [forward, right, forward.cross(right)]
    }

    pub fn position(&self) -> Vec3 {
        self.middle - self.axes()[0] * self.distance.max(1.0)
    }

    /// The ray through `pixel` of a view `size` pixels: where it starts and
    /// the way it goes.
    pub fn ray(&self, pixel: Vec2, size: Vec2) -> (Vec3, Vec3) {
        let [forward, right, up] = self.axes();
        let half = (self.fov() * 0.5).tan();
        let ndc = Vec2::new(pixel.x / size.x * 2.0 - 1.0, 1.0 - pixel.y / size.y * 2.0);
        let direction = forward + right * (ndc.x * half * size.x / size.y) + up * (ndc.y * half);
        (self.position(), direction.normalize())
    }

    /// Where `at`, in cells, shows in a view `size` pixels: none behind it.
    pub fn project(&self, at: Vec3, size: Vec2) -> Option<Vec2> {
        let [forward, right, up] = self.axes();
        let from = at - self.position();
        let depth = from.dot(forward);
        if depth <= 1e-3 {
            return None;
        }
        let half = (self.fov() * 0.5).tan();
        let ndc = Vec2::new(from.dot(right) / (depth * half * size.x / size.y), from.dot(up) / (depth * half));
        Some(Vec2::new((ndc.x + 1.0) * 0.5 * size.x, (1.0 - ndc.y) * 0.5 * size.y))
    }
}

/// How a map is lit and lined.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MapLook {
    /// Towards its lamp, from the view: to its right, up from the ground,
    /// and away from the viewer. The lamp turns with the view, so hills are
    /// lit from the same side however the map is turned.
    pub lamp: Vec3,
    /// How dark its shadows are, 0 for none.
    pub shadows: f32,
    /// How dark blocks' edges are lined, 0 for none.
    pub edges: f32,
}

impl Default for MapLook {
    fn default() -> Self {
        Self { lamp: Vec3::new(-0.6, 0.75, -0.3), shadows: 0.4, edges: 0.6 }
    }
}

/// What a map shows this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MapDraw {
    /// Its image's size, in pixels.
    pub size: [u32; 2],
    /// The window's lowest cell along x and z, a multiple of a tile's side,
    /// and the height its camera's is counted from.
    pub corner: [i32; 2],
    pub base: i32,
    /// Its camera, in cells from `corner` and `base`. The slab is cut round
    /// where it looks.
    pub camera: MapCamera,
    /// How far the slab reaches from its middle, in cells, and whether it
    /// is round.
    pub reach: f32,
    pub round: bool,
    /// The lowest ground and the highest block in the slab, in cells from
    /// `base`, and how thick the slab is under the lowest ground.
    pub heights: [f32; 2],
    pub thick: f32,
    /// Rays a pixel along each side, 1 to 4: more for smoother edges.
    pub samples: u32,
    pub look: MapLook,
}

/// Where a drawn map is, to show it: a texture of its own, premultiplied.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MapImage {
    pub texture: TextureId,
    pub size: [u32; 2],
}

struct View {
    side: u32,
    /// Its columns, then each small group's highest block, then each large
    /// one's: on the GPU alone, as every pixel reads them many times over.
    columns: Buffer,
    palette: Buffer,
    blocks: u32,
    /// What it is to show this frame, and what it was last drawn as.
    draw: Option<MapDraw>,
    drawn: Option<MapDraw>,
    /// Whether its columns or colours changed since it was drawn.
    changed: bool,
    image: Option<MapImage>,
}

#[derive(Default)]
pub(crate) struct Maps {
    next: u32,
    views: HashMap<MapId, View>,
    /// Each frame slot's views.
    slots: Vec<Option<Buffer>>,
    /// Buffers let go, to free once no frame reads them.
    retired: Vec<Buffer>,
    /// What was written since the last frame, to copy into the maps' own
    /// buffers with the next: the bytes, and where each run of them goes.
    staged: Vec<u8>,
    uploads: Vec<Upload>,
}

/// Bytes of `Maps::staged` from `from`, to copy to `to` of a map's buffer.
struct Upload {
    id: MapId,
    from: usize,
    to: u64,
    size: usize,
}

/// Where a map of `side` cells a side keeps its groups' highest blocks, in
/// its buffer.
fn tops_at(side: u32) -> u64 {
    u64::from(side * side) * size_of::<GpuMapColumn>() as u64
}

impl Maps {
    /// A map of `side` cells a side, rounded up to a power of two of at
    /// least a tile, with nothing in it.
    pub fn add(&mut self, gpu: &Gpu, side: u32) -> Result<MapId> {
        let side = side.clamp(MAP_TILE as u32, 2048).next_power_of_two();
        let id = MapId(self.next);
        self.next += 1;
        let groups = u64::from(side / GROUP as u32);
        let large = u64::from(side / LARGE_GROUP as u32);
        let bytes = tops_at(side) + (groups * groups + large * large) * size_of::<i32>() as u64;
        let usage = vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_DST;
        let columns = gpu.create_buffer(&format!("map {} columns", id.0), bytes, usage, MemoryLocation::GpuOnly)?;
        self.stage(id, 0, &vec![GpuMapColumn::NONE; (side * side) as usize]);
        self.stage(id, tops_at(side), &vec![i32::MIN; (groups * groups + large * large) as usize]);
        let palette = palette_buffer(gpu, id, &[[255, 0, 255]])?;
        let view = View { side, columns, palette, blocks: 1, draw: None, drawn: None, changed: true, image: None };
        self.views.insert(id, view);
        Ok(id)
    }

    /// The colour of each block, by its number: sRGB, a byte each.
    pub fn set_palette(&mut self, gpu: &Gpu, id: MapId, colors: &[[u8; 3]]) -> Result<()> {
        let Some(view) = self.views.get_mut(&id) else { return Ok(()) };
        let colors = if colors.is_empty() { &[[255, 0, 255]][..] } else { colors };
        let palette = palette_buffer(gpu, id, colors)?;
        self.retired.push(std::mem::replace(&mut view.palette, palette));
        (view.blocks, view.changed) = (colors.len() as u32, true);
        Ok(())
    }

    /// Writes the tile whose lowest cell is `at`, a multiple of a tile's
    /// side along x and z: its columns a row along x after another.
    pub fn write_tile(&mut self, id: MapId, at: [i32; 2], columns: &[GpuMapColumn]) {
        let Some(view) = self.views.get_mut(&id) else { return };
        let (side, tile) = (view.side as i32, MAP_TILE as i32);
        if columns.len() != MAP_TILE * MAP_TILE || at[0].rem_euclid(tile) != 0 || at[1].rem_euclid(tile) != 0 {
            return;
        }
        view.changed = true;
        for (row, cells) in columns.chunks_exact(MAP_TILE).enumerate() {
            let kept = (at[1] + row as i32).rem_euclid(side) * side + at[0].rem_euclid(side);
            self.stage(id, kept as u64 * size_of::<GpuMapColumn>() as u64, cells);
        }
        // Each group's highest block, a row of groups along x after another.
        let (groups, across) = (MAP_TILE / GROUP, side / GROUP as i32);
        for row in 0..groups {
            let highest = |group: usize| {
                let cells = (0..GROUP * GROUP)
                    .map(|cell| (row * GROUP + cell / GROUP) * MAP_TILE + group * GROUP + cell % GROUP);
                cells.map(|cell| columns[cell].highest).max().unwrap_or(i32::MIN)
            };
            let tops: Vec<i32> = (0..groups).map(highest).collect();
            let from = [at[0].div_euclid(GROUP as i32), at[1].div_euclid(GROUP as i32) + row as i32];
            let kept = from[1].rem_euclid(across) * across + from[0].rem_euclid(across);
            self.stage(id, tops_at(side as u32) + kept as u64 * size_of::<i32>() as u64, &tops);
        }
        // The tile's own, after every small group's.
        let highest = columns.iter().map(|column| column.highest).max().unwrap_or(i32::MIN);
        let (large, tiles) = (at.map(|cell| cell.div_euclid(tile)), side / tile);
        let kept = across * across + large[1].rem_euclid(tiles) * tiles + large[0].rem_euclid(tiles);
        self.stage(id, tops_at(side as u32) + kept as u64 * size_of::<i32>() as u64, &[highest]);
    }

    /// Keeps `data` to copy to `to` of map `id`'s buffer with the next
    /// frame.
    fn stage<T: bytemuck::Pod>(&mut self, id: MapId, to: u64, data: &[T]) {
        let bytes: &[u8] = bytemuck::cast_slice(data);
        self.uploads.push(Upload { id, from: self.staged.len(), to, size: bytes.len() });
        self.staged.extend_from_slice(bytes);
    }

    /// Asks for the map to be drawn this frame, as `draw` says. Asked each
    /// frame it shows; drawn again only when something changed.
    pub fn draw(&mut self, id: MapId, draw: MapDraw) {
        if let Some(view) = self.views.get_mut(&id) {
            view.draw = Some(draw);
        }
    }

    /// Where the map was last drawn, once it has been.
    pub fn image(&self, id: MapId) -> Option<MapImage> {
        self.views.get(&id)?.image
    }

    /// The window's cells along a side.
    pub fn side(&self, id: MapId) -> Option<u32> {
        Some(self.views.get(&id)?.side)
    }

    pub fn remove(&mut self, id: MapId) {
        if let Some(view) = self.views.remove(&id) {
            self.retired.extend([view.columns, view.palette]);
        }
    }

    /// Adds this frame's map passes to `fg`, and returns the images the UI
    /// may sample.
    pub fn record(
        &mut self,
        fg: &mut FrameGraph<'_>,
        (gpu, bindless): (&Gpu, &mut Bindless),
        pipelines: &Pipelines,
        slot: usize,
    ) -> Result<Vec<ImageId>> {
        // What was written since last frame goes into the maps' buffers first.
        let mut written: HashMap<MapId, crate::graph::BufferId> = HashMap::new();
        if !self.staged.is_empty() {
            let usage = vk::BufferUsageFlags::TRANSFER_SRC;
            let bytes = self.staged.len() as u64;
            let staging = gpu.create_buffer("map uploads", bytes, usage, MemoryLocation::CpuToGpu)?;
            staging.write(0, &self.staged);
            let mut copies: Vec<(vk::Buffer, vk::BufferCopy)> = Vec::with_capacity(self.uploads.len());
            for upload in self.uploads.drain(..) {
                let Some(view) = self.views.get(&upload.id) else { continue };
                written.entry(upload.id).or_insert_with(|| fg.import_buffer(&view.columns));
                let region =
                    vk::BufferCopy { src_offset: upload.from as u64, dst_offset: upload.to, size: upload.size as u64 };
                copies.push((view.columns.handle, region));
            }
            let targets: Vec<_> = written.values().map(|&id| (id, BufferAccess::TransferWrite)).collect();
            let source = staging.handle;
            fg.add_pass("Map uploads", &[], &targets, move |ctx| {
                for (target, region) in &copies {
                    unsafe { ctx.gpu.device.cmd_copy_buffer(ctx.cmd, source, *target, std::slice::from_ref(region)) };
                }
            });
            self.staged.clear();
            self.retired.push(staging);
        }
        let mut shown: Vec<MapId> =
            self.views.iter().filter(|(_, view)| view.draw.is_some()).map(|(id, _)| *id).collect();
        if shown.is_empty() {
            return Ok(Vec::new());
        }
        shown.sort_by_key(|id| id.0);
        shown.truncate(MAX_VIEWS);
        let views_at = self.slot_buffer(gpu, slot)?.address;
        let mut frames: Vec<GpuMapView> = Vec::with_capacity(shown.len());
        let mut images = Vec::with_capacity(shown.len());
        for id in shown {
            let view = self.views.get_mut(&id).expect("listed above");
            let draw = view.draw.take().expect("filtered on it");
            let size = draw.size.map(|pixels| pixels.clamp(1, MAX_MAP_SIZE));
            let usage = vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::SAMPLED;
            let desc = ImageDesc::new(size[0], size[1], HDR_FORMAT, usage);
            let image = fg.history_image(gpu, bindless, &format!("map {}", id.0), desc)?;
            images.push(image);
            view.image = Some(MapImage { texture: fg.texture(image), size });
            if !fg.is_new(image) && !view.changed && view.drawn == Some(draw) {
                continue;
            }
            (view.drawn, view.changed) = (Some(draw), false);
            let frame = views_at + (frames.len() * size_of::<GpuMapView>()) as u64;
            frames.push(gpu_view(&draw, size, view.side, view.blocks));
            let (palette, tops) = (view.palette.address, view.columns.address + tops_at(view.side));
            let push = Push {
                frame,
                buffers: view.columns.address,
                texture: palette as u32,
                max_draws: (palette >> 32) as u32,
                list: tops as u32,
                output: (tops >> 32) as u32,
                ..Default::default()
            };
            let read = *written.entry(id).or_insert_with(|| fg.import_buffer(&view.columns));
            let pipeline = pipelines.map;
            let extent = vk::Extent2D { width: size[0], height: size[1] };
            let buffers = [(read, BufferAccess::ShaderRead)];
            fg.add_pass("Map", &[(image, ImageAccess::ColorAttachment)], &buffers, move |ctx| {
                let clear = vk::ClearValue { color: vk::ClearColorValue { float32: [0.0; 4] } };
                let color = [attachment(ctx.view(image), vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL, Some(clear))];
                begin_rendering(ctx, extent, &color, None);
                bind(ctx, pipeline, vk::PipelineBindPoint::GRAPHICS, &push);
                unsafe {
                    ctx.gpu.device.cmd_draw(ctx.cmd, 3, 1, 0, 0);
                    ctx.gpu.device.cmd_end_rendering(ctx.cmd);
                }
            });
        }
        if !frames.is_empty() {
            self.slot_buffer(gpu, slot)?.write(0, &frames);
        }
        Ok(images)
    }

    fn slot_buffer(&mut self, gpu: &Gpu, slot: usize) -> Result<&Buffer> {
        if self.slots.len() <= slot {
            self.slots.resize_with(slot + 1, || None);
        }
        if self.slots[slot].is_none() {
            let bytes = (MAX_VIEWS * size_of::<GpuMapView>()) as u64;
            let storage = vk::BufferUsageFlags::STORAGE_BUFFER;
            let buffer = gpu.create_buffer(&format!("map views {slot}"), bytes, storage, MemoryLocation::CpuToGpu)?;
            self.slots[slot] = Some(buffer);
        }
        Ok(self.slots[slot].as_ref().expect("made above"))
    }

    /// The buffers let go since last asked, to free once no frame reads
    /// them.
    pub fn take_retired(&mut self) -> Vec<Buffer> {
        std::mem::take(&mut self.retired)
    }

    /// Every buffer, as the renderer goes.
    pub fn take_buffers(&mut self) -> Vec<Buffer> {
        let views = self.views.drain().flat_map(|(_, view)| [view.columns, view.palette]);
        let all: Vec<Buffer> = views.chain(self.slots.drain(..).flatten()).collect();
        all.into_iter().chain(self.take_retired()).collect()
    }
}

fn palette_buffer(gpu: &Gpu, id: MapId, colors: &[[u8; 3]]) -> Result<Buffer> {
    let packed: Vec<u32> = colors.iter().map(|[r, g, b]| u32::from_le_bytes([*r, *g, *b, 255])).collect();
    let storage = vk::BufferUsageFlags::STORAGE_BUFFER;
    let bytes = (packed.len() * size_of::<u32>()) as u64;
    let buffer = gpu.create_buffer(&format!("map {} palette", id.0), bytes, storage, MemoryLocation::CpuToGpu)?;
    buffer.write(0, &packed);
    Ok(buffer)
}

fn gpu_view(draw: &MapDraw, size: [u32; 2], side: u32, blocks: u32) -> GpuMapView {
    let camera = draw.camera;
    let [forward, right, up] = camera.axes();
    let half = (camera.fov() * 0.5).tan();
    let aspect = size[0] as f32 / size[1] as f32;
    // The lamp, given from the view, in the map's own directions: its right, up from the ground, and away.
    let away = Vec3::new(forward.x, 0.0, forward.z).normalize_or(Vec3::Z);
    let lamp = (right * draw.look.lamp.x + Vec3::Y * draw.look.lamp.y + away * draw.look.lamp.z).normalize_or(Vec3::Y);
    let [lowest, highest] = draw.heights;
    GpuMapView {
        origin: camera.position().to_array(),
        reach: draw.reach.max(1.0),
        forward: forward.to_array(),
        bottom: lowest - draw.thick.max(0.0),
        right: (right * half * aspect).to_array(),
        ceiling: highest.max(lowest + 1.0) + 0.01,
        up: (up * half).to_array(),
        samples: draw.samples.clamp(1, 4),
        middle: [camera.middle.x, camera.middle.z],
        size: size.map(|pixels| pixels as f32),
        corner: draw.corner,
        base: draw.base,
        side,
        sun: lamp.to_array(),
        shadows: draw.look.shadows.clamp(0.0, 1.0),
        round: u32::from(draw.round),
        blocks: blocks.max(1),
        edges: draw.look.edges.clamp(0.0, 1.0),
        lowest,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_maps_gpu_structs_are_laid_out_as_its_shader_reads_them() {
        assert_eq!(size_of::<GpuMapColumn>(), 40);
        assert_eq!(size_of::<GpuMapView>(), 128);
    }

    #[test]
    fn a_column_packs_its_blocks_and_pieces_for_the_shader() {
        let column = MapColumn {
            top: 10,
            blocks: [3, 2, 1],
            depths: [1, 3],
            water: 12,
            water_block: 4,
            pieces: [MapPiece { from: 14, to: 17, top: 6, side: 5 }, MapPiece::NONE],
        };
        let packed = GpuMapColumn::of(&column);
        assert_eq!((packed.top, packed.water, packed.highest), (10, 12, 17));
        assert_eq!((packed.layers, packed.deep, packed.depths), (3 | 2 << 16, 1 | 4 << 16, 1 | 3 << 8));
        assert_eq!((packed.piece0, packed.piece0_blocks, packed.piece1), (4 | 7 << 16, 6 | 5 << 16, 0));
        assert_eq!(GpuMapColumn::of(&MapColumn::NONE), GpuMapColumn::NONE);
    }

    #[test]
    fn a_map_camera_looks_north_with_east_to_its_right_and_sees_what_it_looks_at_in_its_middle() {
        let size = Vec2::new(400.0, 200.0);
        let camera = MapCamera { middle: Vec3::new(50.0, 4.0, 60.0), turn: 0.0, tilt: 0.9, distance: 80.0, fov: 0.5 };
        let middle = camera.project(camera.middle, size).unwrap();
        assert!((middle - size * 0.5).length() < 1e-3, "{middle:?}");
        let east = camera.project(camera.middle + Vec3::X * 5.0, size).unwrap();
        let north = camera.project(camera.middle + Vec3::Z * 5.0, size).unwrap();
        assert!(east.x > middle.x + 5.0 && (east.y - middle.y).abs() < 1e-2, "east is to the right: {east:?}");
        assert!(north.y < middle.y - 5.0 && (north.x - middle.x).abs() < 1e-2, "north is up: {north:?}");
        // A pixel's ray goes back through what showed there.
        let (origin, direction) = camera.ray(east, size);
        let to = camera.middle + Vec3::X * 5.0 - origin;
        assert!(direction.dot(to.normalize()) > 0.99999);
        assert!(camera.ray(Vec2::new(0.0, 0.0), size).1.y < 0.0, "every ray goes down");
        // Turned a quarter, east is up the view.
        let turned = MapCamera { turn: std::f32::consts::FRAC_PI_2, ..camera };
        let east = turned.project(camera.middle + Vec3::X * 5.0, size).unwrap();
        assert!(east.y < middle.y - 5.0, "{east:?}");
    }
}
