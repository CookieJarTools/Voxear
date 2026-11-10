//! Acceleration structures, for ray queries where the GPU has ray tracing:
//! a bottom level for each mesh, built once from its triangles where they
//! already lie on the GPU, and a top level for each frame, rebuilt from the
//! instances drawn, which shaders find in the bindless set's `scenes`.

use anyhow::{Context, Result};
use ash::vk;
use gpu_allocator::MemoryLocation;

use crate::{device::Gpu, resources::Buffer};

/// An acceleration structure and the buffer it lives in.
pub struct AccelerationStructure {
    pub handle: vk::AccelerationStructureKHR,
    /// What instances in a top level point at.
    pub address: vk::DeviceAddress,
    buffer: Buffer,
}

/// A bottom level waiting to be built: into which structure, from which
/// triangles, with which scratch space.
pub(crate) struct BottomLevelBuild {
    pub structure: vk::AccelerationStructureKHR,
    pub triangles: Triangles,
    pub scratch: Buffer,
    pub scratch_address: vk::DeviceAddress,
}

/// A mesh's triangles as a bottom level's one geometry.
fn bottom_level_geometry(triangles: &Triangles) -> vk::AccelerationStructureGeometryKHR<'static> {
    let data = vk::AccelerationStructureGeometryTrianglesDataKHR::default()
        .vertex_format(vk::Format::R32G32B32_SFLOAT)
        .vertex_data(vk::DeviceOrHostAddressConstKHR { device_address: triangles.vertices })
        .vertex_stride(triangles.stride)
        .max_vertex(triangles.vertex_count.saturating_sub(1))
        .index_type(vk::IndexType::UINT32)
        .index_data(vk::DeviceOrHostAddressConstKHR { device_address: triangles.indices });
    vk::AccelerationStructureGeometryKHR::default()
        .geometry_type(vk::GeometryTypeKHR::TRIANGLES)
        .geometry(vk::AccelerationStructureGeometryDataKHR { triangles: data })
        .flags(vk::GeometryFlagsKHR::OPAQUE)
}

/// How a bottom level of `geometry` is built.
fn bottom_level_info<'a>(
    geometry: &'a [vk::AccelerationStructureGeometryKHR<'a>],
) -> vk::AccelerationStructureBuildGeometryInfoKHR<'a> {
    vk::AccelerationStructureBuildGeometryInfoKHR::default()
        .ty(vk::AccelerationStructureTypeKHR::BOTTOM_LEVEL)
        .flags(vk::BuildAccelerationStructureFlagsKHR::PREFER_FAST_TRACE)
        .mode(vk::BuildAccelerationStructureModeKHR::BUILD)
        .geometries(geometry)
}

/// A mesh's triangles where they lie on the GPU: positions as the first
/// three floats of each `stride`-byte vertex, and three `u32` indices a
/// triangle, counting from the first vertex.
#[derive(Clone, Copy, Debug)]
pub struct Triangles {
    pub vertices: vk::DeviceAddress,
    pub stride: u64,
    pub vertex_count: u32,
    pub indices: vk::DeviceAddress,
    pub triangle_count: u32,
}

/// One instance of a mesh in a top level: its bottom level, where it is (the
/// first three rows of its model matrix, row by row), the number shaders
/// get back for it, `CommittedInstanceID`, and whether it is opaque. Ray
/// queries are handed the triangles of one that is not as candidates, to
/// keep or pass through, as for a cut-out material's holes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Instance {
    pub mesh: vk::DeviceAddress,
    pub transform: [f32; 12],
    pub id: u32,
    pub opaque: bool,
}

/// The usage buffers holding a bottom level's triangles need.
pub const BUILD_INPUT: vk::BufferUsageFlags = vk::BufferUsageFlags::ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY_KHR;

impl Gpu {
    fn loader(&self) -> &ash::khr::acceleration_structure::Device {
        self.acceleration_structure.as_ref().expect("ray tracing is not on")
    }

    /// A scratch buffer of at least `size` bytes, its address aligned as
    /// builds need.
    fn scratch(&self, name: &str, size: u64) -> Result<(Buffer, vk::DeviceAddress)> {
        let mut properties = vk::PhysicalDeviceAccelerationStructurePropertiesKHR::default();
        let mut all = vk::PhysicalDeviceProperties2::default().push_next(&mut properties);
        unsafe { self.instance.get_physical_device_properties2(self.physical, &mut all) };
        let align = u64::from(properties.min_acceleration_structure_scratch_offset_alignment).max(1);
        let usage = vk::BufferUsageFlags::STORAGE_BUFFER;
        let buffer = self.create_buffer(name, size + align, usage, MemoryLocation::GpuOnly)?;
        let address = buffer.address.next_multiple_of(align);
        Ok((buffer, address))
    }

    fn create_structure(
        &self,
        name: &str,
        kind: vk::AccelerationStructureTypeKHR,
        size: u64,
    ) -> Result<AccelerationStructure> {
        let usage = vk::BufferUsageFlags::ACCELERATION_STRUCTURE_STORAGE_KHR;
        let buffer = self.create_buffer(name, size, usage, MemoryLocation::GpuOnly)?;
        let info = vk::AccelerationStructureCreateInfoKHR::default().buffer(buffer.handle).size(size).ty(kind);
        let handle = unsafe { self.loader().create_acceleration_structure(&info, None) }
            .with_context(|| format!("creating the acceleration structure {name}"))?;
        let address = unsafe {
            self.loader().get_acceleration_structure_device_address(
                &vk::AccelerationStructureDeviceAddressInfoKHR::default().acceleration_structure(handle),
            )
        };
        Ok(AccelerationStructure { handle, address, buffer })
    }

    /// Builds a mesh's bottom level from its triangles, and waits for it; while
    /// a batch is open, when it closes (see [`Gpu::batch`]).
    pub fn build_bottom_level(&self, name: &str, triangles: Triangles) -> Result<AccelerationStructure> {
        let geometry = [bottom_level_geometry(&triangles)];
        let info = bottom_level_info(&geometry);
        let mut sizes = vk::AccelerationStructureBuildSizesInfoKHR::default();
        unsafe {
            self.loader().get_acceleration_structure_build_sizes(
                vk::AccelerationStructureBuildTypeKHR::DEVICE,
                &info,
                &[triangles.triangle_count],
                &mut sizes,
            )
        };
        let structure = self.create_structure(
            name,
            vk::AccelerationStructureTypeKHR::BOTTOM_LEVEL,
            sizes.acceleration_structure_size,
        )?;
        let (scratch, scratch_address) = self.scratch("bottom level scratch", sizes.build_scratch_size)?;
        let build = BottomLevelBuild { structure: structure.handle, triangles, scratch, scratch_address };
        {
            let mut batch = self.batch.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            if batch.open > 0 {
                batch.builds.push(build);
                return Ok(structure);
            }
        }
        let builds = [build];
        self.immediate(|cmd| self.record_bottom_levels(cmd, &builds))?;
        let [build] = builds;
        self.destroy_buffer(build.scratch);
        Ok(structure)
    }

    /// Records the building of bottom levels, all at once.
    pub(crate) fn record_bottom_levels(&self, cmd: vk::CommandBuffer, builds: &[BottomLevelBuild]) {
        let geometries: Vec<[vk::AccelerationStructureGeometryKHR; 1]> =
            builds.iter().map(|build| [bottom_level_geometry(&build.triangles)]).collect();
        let infos: Vec<vk::AccelerationStructureBuildGeometryInfoKHR> = builds
            .iter()
            .zip(&geometries)
            .map(|(build, geometry)| {
                bottom_level_info(geometry)
                    .dst_acceleration_structure(build.structure)
                    .scratch_data(vk::DeviceOrHostAddressKHR { device_address: build.scratch_address })
            })
            .collect();
        let ranges: Vec<[vk::AccelerationStructureBuildRangeInfoKHR; 1]> = builds
            .iter()
            .map(|build| {
                [vk::AccelerationStructureBuildRangeInfoKHR::default().primitive_count(build.triangles.triangle_count)]
            })
            .collect();
        let ranges: Vec<&[vk::AccelerationStructureBuildRangeInfoKHR]> =
            ranges.iter().map(|range| &range[..]).collect();
        unsafe { self.loader().cmd_build_acceleration_structures(cmd, &infos, &ranges) };
    }

    pub fn destroy_acceleration_structure(&self, structure: AccelerationStructure) {
        unsafe { self.loader().destroy_acceleration_structure(structure.handle, None) };
        self.destroy_buffer(structure.buffer);
    }
}

/// A top level of up to `capacity` instances, rebuilt each frame it is used,
/// with its scratch space and the instances written from the CPU.
pub struct TopLevel {
    pub structure: AccelerationStructure,
    scratch: Buffer,
    scratch_address: vk::DeviceAddress,
    instances: Buffer,
    capacity: u32,
    count: u32,
}

/// A top level's build, as [`TopLevel::build`] gives it.
#[derive(Clone, Copy, Debug)]
pub struct TopLevelBuild {
    structure: vk::AccelerationStructureKHR,
    instances: vk::DeviceAddress,
    scratch: vk::DeviceAddress,
    count: u32,
}

impl TopLevelBuild {
    /// Records the build. Ray queries that read it must wait for
    /// `ACCELERATION_STRUCTURE_BUILD` and `ACCELERATION_STRUCTURE_WRITE`.
    pub fn record(self, gpu: &Gpu, cmd: vk::CommandBuffer) {
        let geometry = TopLevel::geometry(self.instances);
        let info = TopLevel::build_info(&geometry)
            .dst_acceleration_structure(self.structure)
            .scratch_data(vk::DeviceOrHostAddressKHR { device_address: self.scratch });
        let range = [vk::AccelerationStructureBuildRangeInfoKHR::default().primitive_count(self.count)];
        unsafe { gpu.loader().cmd_build_acceleration_structures(cmd, &[info], &[&range]) };
    }
}

impl TopLevel {
    pub fn new(gpu: &Gpu, name: &str, capacity: u32) -> Result<Self> {
        let instance_size = size_of::<vk::AccelerationStructureInstanceKHR>() as u64;
        let usage = BUILD_INPUT;
        let instances =
            gpu.create_buffer(name, instance_size * u64::from(capacity.max(1)), usage, MemoryLocation::CpuToGpu)?;
        let build = Self::geometry(instances.address);
        let info = Self::build_info(&build);
        let mut sizes = vk::AccelerationStructureBuildSizesInfoKHR::default();
        unsafe {
            gpu.loader().get_acceleration_structure_build_sizes(
                vk::AccelerationStructureBuildTypeKHR::DEVICE,
                &info,
                &[capacity],
                &mut sizes,
            )
        };
        let structure =
            gpu.create_structure(name, vk::AccelerationStructureTypeKHR::TOP_LEVEL, sizes.acceleration_structure_size)?;
        let (scratch, scratch_address) = gpu.scratch("top level scratch", sizes.build_scratch_size)?;
        Ok(Self { structure, scratch, scratch_address, instances, capacity, count: 0 })
    }

    fn geometry(instances: vk::DeviceAddress) -> [vk::AccelerationStructureGeometryKHR<'static>; 1] {
        let data = vk::AccelerationStructureGeometryInstancesDataKHR::default()
            .data(vk::DeviceOrHostAddressConstKHR { device_address: instances });
        [vk::AccelerationStructureGeometryKHR::default()
            .geometry_type(vk::GeometryTypeKHR::INSTANCES)
            .geometry(vk::AccelerationStructureGeometryDataKHR { instances: data })]
    }

    fn build_info<'a>(
        geometry: &'a [vk::AccelerationStructureGeometryKHR<'a>],
    ) -> vk::AccelerationStructureBuildGeometryInfoKHR<'a> {
        vk::AccelerationStructureBuildGeometryInfoKHR::default()
            .ty(vk::AccelerationStructureTypeKHR::TOP_LEVEL)
            .flags(vk::BuildAccelerationStructureFlagsKHR::PREFER_FAST_BUILD)
            .mode(vk::BuildAccelerationStructureModeKHR::BUILD)
            .geometries(geometry)
    }

    /// Writes this frame's instances, as many as fit; the build records them.
    pub fn write(&mut self, instances: &[Instance]) {
        let written: Vec<vk::AccelerationStructureInstanceKHR> = instances
            .iter()
            .take(self.capacity as usize)
            .map(|instance| {
                let mut flags = vk::GeometryInstanceFlagsKHR::TRIANGLE_FACING_CULL_DISABLE;
                if !instance.opaque {
                    flags |= vk::GeometryInstanceFlagsKHR::FORCE_NO_OPAQUE;
                }
                vk::AccelerationStructureInstanceKHR {
                    transform: vk::TransformMatrixKHR { matrix: instance.transform },
                    instance_custom_index_and_mask: vk::Packed24_8::new(instance.id, 0xFF),
                    instance_shader_binding_table_record_offset_and_flags: vk::Packed24_8::new(0, flags.as_raw() as u8),
                    acceleration_structure_reference: vk::AccelerationStructureReferenceKHR {
                        device_handle: instance.mesh,
                    },
                }
            })
            .collect();
        let mapped = self.instances.mapped().expect("instances are host visible");
        let bytes = written.len() * size_of::<vk::AccelerationStructureInstanceKHR>();
        // The instance struct is plain data as Vulkan lays it out.
        unsafe { std::ptr::copy_nonoverlapping(written.as_ptr().cast::<u8>(), mapped.as_ptr(), bytes) };
        self.count = written.len() as u32;
    }

    /// The build of what `write` last wrote, to record when the frame's
    /// commands are.
    pub fn build(&self) -> TopLevelBuild {
        TopLevelBuild {
            structure: self.structure.handle,
            instances: self.instances.address,
            scratch: self.scratch_address,
            count: self.count,
        }
    }

    pub fn destroy(self, gpu: &Gpu) {
        gpu.destroy_acceleration_structure(self.structure);
        gpu.destroy_buffer(self.scratch);
        gpu.destroy_buffer(self.instances);
    }
}
