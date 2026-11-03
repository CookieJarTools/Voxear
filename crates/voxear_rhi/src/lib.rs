//! Vulkan on `ash`: devices, resources, bindless descriptors, pipelines and
//! frame pacing.
//!
//! The baseline is Vulkan 1.3 with dynamic rendering, synchronization2,
//! timeline semaphores, buffer device address, scalar layouts, indirect draw
//! count, descriptor indexing (bindless) and wave ballots in compute shaders.
//! That covers NVIDIA, AMD and Intel GPUs from roughly 2016 on, on Linux and
//! Windows. Mesh shaders and ray tracing are turned on where the GPU has them.

mod accel;
mod bindless;
mod device;
mod frame;
mod pipeline;
mod renderdoc;
mod resources;
mod swapchain;
mod timer;

pub use ash::vk;
pub use accel::{AccelerationStructure, BUILD_INPUT, Instance, TopLevel, TopLevelBuild, Triangles};
pub use bindless::{Bindless, MAX_SCENES, MAX_TEXTURES, PUSH_CONSTANT_SIZE, StorageImageId, TextureId, samplers};
pub use device::{Gpu, GpuInfo, WindowHandles, enumerate_gpus};
pub use frame::{Acquire, Acquired, FRAMES_IN_FLIGHT, Frame, Frames};
pub use gpu_allocator::MemoryLocation;
pub use pipeline::{Blend, GraphicsPipelineDesc, ShaderModule, create_compute_pipeline, create_graphics_pipeline};
pub use renderdoc::RenderDoc;
pub use resources::{
    Buffer, GpuMemory, Image, ImageDesc, Retired, Transition, full_mip_count, is_depth, mip_barrier,
};
pub use swapchain::Swapchain;
pub use timer::GpuTimer;
