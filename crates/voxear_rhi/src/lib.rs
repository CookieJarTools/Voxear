//! Vulkan on `ash`: device selection, swapchain and frame pacing.
//!
//! The baseline is Vulkan 1.3 with dynamic rendering, synchronization2,
//! timeline semaphores, buffer device address and descriptor indexing
//! (bindless). That covers NVIDIA, AMD and Intel GPUs from roughly 2016 on,
//! on Linux and Windows. Mesh shaders and ray tracing are detected and
//! reported as optional tiers.

mod device;
mod frame;
mod swapchain;

pub use ash::vk;
pub use device::{Gpu, GpuInfo, enumerate_gpus};
pub use frame::{FRAMES_IN_FLIGHT, Frame, Frames, transition_image};
pub use swapchain::Swapchain;
