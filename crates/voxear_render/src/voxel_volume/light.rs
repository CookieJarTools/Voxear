//! The light kept round the camera for Voxel Volumes: a cell a brick of
//! the finest volume, half a metre for voxels of 1/16 m, each with how
//! much of the sky's light reaches it and how much of its voxels' glow, as
//! the light kept for blocks is a byte a block.
//!
//! Each frame, what lets light through each cell is worked out from its
//! brick's voxels on the GPU (`voxelLightCells` in `shaders/voxels.slang`),
//! and which cells the sky reaches straight down (`voxelLightSky`); then the
//! light is spread a few steps from last frame's, moved as the camera moved
//! (`voxelLightSpread`), a step less a cell each way from where it is. A change to the
//! voxels lights the cells round it again over the next frames.
//!
//! The voxels, and models, blocks and grass beside them, read it as they
//! read the light kept for blocks (`voxelLightAround` in
//! `shaders/common.slang`): so a cellar, or a room under a roof, is dark but
//! for its doors, its windows and the voxels that glow in it.

use anyhow::Result;
use voxear_core::math::{IVec3, Vec3};
use voxear_rhi::{Buffer, Gpu, MemoryLocation, vk};

use super::GpuVoxelVolume;
use crate::{
    gpu::Push,
    graph::{BufferAccess, BufferId, FrameGraph},
    passes::bind,
    pipelines::Pipelines,
};

/// How many cells the light is kept for along X, Y and Z, round the camera:
/// 48 by 24 by 48 m for voxels of 1/16 m.
pub(crate) const LIGHT_CELLS: [i32; 3] = [96, 48, 96];
/// How many steps the light is spread a frame, and on a frame with no light
/// from the last to start from: as far as the brightest light reaches.
const STEPS: u32 = 4;
const FIRST_STEPS: u32 = 32;

/// The light kept round the camera as the shaders read it (`VoxelLightGrid`
/// in `shaders/voxels.slang`).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
struct GpuLightGrid {
    volumes: u64,
    light_a: u64,
    light_b: u64,
    cells: u64,
    corner: [f32; 3],
    cell: f32,
    size: [i32; 3],
    pad: u32,
    moved: [i32; 3],
    kept: u32,
}

/// What a view reads of the light kept round the camera this frame.
#[derive(Clone, Copy, Debug)]
pub(crate) struct LightView {
    pub light: BufferId,
    pub cells: BufferId,
    pub light_address: u64,
    pub cells_address: u64,
    pub corner: Vec3,
    pub cell: f32,
}

/// The light kept round the camera, and each frame slot's record of it.
pub(crate) struct LightGrid {
    light: [Buffer; 2],
    cells: Buffer,
    /// Each frame slot's record and its list of volumes.
    slots: Vec<(Buffer, Buffer)>,
    /// The corner of the lattice of cells last frame's were on, how wide a
    /// cell was, and where cell 0 was on it; and which volumes it was lit
    /// from, by where their voxels are on the GPU.
    last: Option<(Vec3, f32, IVec3)>,
    lit_from: Vec<u64>,
}

impl LightGrid {
    pub fn new(gpu: &Gpu) -> Result<Self> {
        let count = (LIGHT_CELLS[0] * LIGHT_CELLS[1] * LIGHT_CELLS[2]) as u64;
        let usage = vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS;
        let light = [
            gpu.create_buffer("voxel light a", count * 4, usage, MemoryLocation::GpuOnly)?,
            gpu.create_buffer("voxel light b", count * 4, usage, MemoryLocation::GpuOnly)?,
        ];
        let cells = gpu.create_buffer("voxel light cells", count * 8, usage, MemoryLocation::GpuOnly)?;
        Ok(Self { light, cells, slots: Vec::new(), last: None, lit_from: Vec::new() })
    }

    /// Records the passes that light the cells round `camera` from the
    /// volumes `drawn`, for frame slot `slot`, after what `pools` makes.
    pub fn record(
        &mut self,
        gpu: &Gpu,
        fg: &mut FrameGraph<'_>,
        pipelines: Pipelines,
        (slot, pools): (usize, &[BufferId]),
        drawn: &[(GpuVoxelVolume, Vec3)],
        camera: Vec3,
    ) -> Result<Option<LightView>> {
        // The cells are the bricks of the finest volume.
        let Some((finest, corner)) = drawn.iter().min_by(|a, b| a.0.voxel.total_cmp(&b.0.voxel)) else {
            self.last = None;
            return Ok(None);
        };
        let cell = finest.voxel * 8.0;
        let half = IVec3::from_array(LIGHT_CELLS) / 2;
        let origin = ((camera - *corner) / cell).floor().as_ivec3() - half;
        // Other volumes start it afresh: last frame's light was of other voxels.
        let mut lit_from: Vec<u64> = drawn.iter().map(|(record, _)| record.sectors).collect();
        lit_from.sort_unstable();
        let same = lit_from == self.lit_from;
        self.lit_from = lit_from;
        let kept = self.last.filter(|(at, size, _)| same && at == corner && *size == cell);
        let moved = kept.map_or(IVec3::ZERO, |(_, _, last)| origin - last);
        self.last = Some((*corner, cell, origin));
        while self.slots.len() <= slot {
            let usage = vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS;
            let n = self.slots.len();
            let record = gpu.create_buffer(&format!("voxel light {n}"), 256, usage, MemoryLocation::CpuToGpu)?;
            let list = gpu.create_buffer(
                &format!("voxel light volumes {n}"),
                super::FRAME_SIZE,
                usage,
                MemoryLocation::CpuToGpu,
            )?;
            self.slots.push((record, list));
        }
        let (record, list) = &self.slots[slot];
        list.write(0, std::slice::from_ref(&super::volumes_record(drawn, Vec3::ZERO)));
        let at = *corner + origin.as_vec3() * cell;
        let grid = GpuLightGrid {
            volumes: list.address,
            light_a: self.light[0].address,
            light_b: self.light[1].address,
            cells: self.cells.address,
            corner: at.to_array(),
            cell,
            size: LIGHT_CELLS,
            pad: 0,
            // Cell `i` now was cell `i + moved` last frame.
            moved: moved.to_array(),
            kept: u32::from(kept.is_some()),
        };
        record.write(0, std::slice::from_ref(&grid));
        let address = record.address;
        let push = Push { textures: [address as u32, (address >> 32) as u32, 0, 0], ..Push::default() };
        let ids = [fg.import_buffer(&self.light[0]), fg.import_buffer(&self.light[1])];
        let cells = fg.import_buffer(&self.cells);
        let groups = LIGHT_CELLS.map(|n| (n as u32).div_ceil(4));
        let mut reads: Vec<_> = pools.iter().map(|&pool| (pool, BufferAccess::ShaderRead)).collect();
        reads.push((cells, BufferAccess::ComputeWrite));
        fg.add_pass("Voxel light cells", &[], &reads, move |ctx| {
            bind(ctx, pipelines.voxel_light_cells, vk::PipelineBindPoint::COMPUTE, &push);
            unsafe { ctx.gpu.device.cmd_dispatch(ctx.cmd, groups[0], groups[1], groups[2]) };
        });
        let columns = [(LIGHT_CELLS[0] as u32).div_ceil(8), (LIGHT_CELLS[2] as u32).div_ceil(8)];
        fg.add_pass("Voxel light sky", &[], &[(cells, BufferAccess::ComputeWrite)], move |ctx| {
            bind(ctx, pipelines.voxel_light_sky, vk::PipelineBindPoint::COMPUTE, &push);
            unsafe { ctx.gpu.device.cmd_dispatch(ctx.cmd, columns[0], 1, columns[1]) };
        });
        let steps = if kept.is_some() { STEPS } else { FIRST_STEPS };
        for step in 0..steps {
            let (from, to) = (ids[(step % 2) as usize], ids[((step + 1) % 2) as usize]);
            let access =
                [(cells, BufferAccess::ShaderRead), (from, BufferAccess::ShaderRead), (to, BufferAccess::ComputeWrite)];
            let push = Push { level: step % 2, list: u32::from(step == 0), ..push };
            fg.add_pass("Voxel light spread", &[], &access, move |ctx| {
                bind(ctx, pipelines.voxel_light_spread, vk::PipelineBindPoint::COMPUTE, &push);
                unsafe { ctx.gpu.device.cmd_dispatch(ctx.cmd, groups[0], groups[1], groups[2]) };
            });
        }
        Ok(Some(LightView {
            light: ids[0],
            cells,
            light_address: self.light[0].address,
            cells_address: self.cells.address,
            corner: at,
            cell,
        }))
    }

    pub fn into_buffers(self) -> Vec<Buffer> {
        let mut all: Vec<Buffer> = self.light.into_iter().collect();
        all.push(self.cells);
        all.extend(self.slots.into_iter().flat_map(|(record, list)| [record, list]));
        all
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_light_grid_is_laid_out_as_the_shaders_read_it() {
        assert_eq!(size_of::<GpuLightGrid>(), 80);
        // Spread an even number of steps, so the light ends where it started, which the views read.
        assert!(STEPS % 2 == 0 && FIRST_STEPS % 2 == 0);
        assert!(size_of::<super::super::GpuVoxelVolumes>() as u64 == super::super::FRAME_SIZE);
    }
}
