//! Ray-traced global illumination: probes round the camera that trace rays
//! against the scene's triangles, as RTXGI and DDGI do, by ray tracing
//! where the GPU has it and in a compute shader where it does not. The
//! shaders are `shaders/ddgi.slang`, which traces and updates the probes,
//! and `shaders/ddgi_sample.slang`, which shading reads them with.
//!
//! Each probe keeps, in small octahedral maps, the light reaching it from
//! every direction and how far its rays went before they hit something.
//! Shading takes light from the probes round a point only as far as their
//! rays say they can see it, so a probe on the far side of a wall gives
//! none, and probes whose rays find mostly the backs of surfaces are inside
//! something and are set aside. Light does not leak through walls.
//!
//! Each frame:
//!
//! 1. **Scene BVH** builds the frame's top level from the instances drawn.
//! 2. **DDGI rays** traces 256 rays from each probe due: a quarter of the
//!    nearest cascade's probes, an eighth of the next and a sixteenth of the
//!    farthest, scattered, every probe a grid has just moved over, and all of
//!    them while the GI warms up.
//! 3. **DDGI update** blends those rays into the due probes' maps.
//!
//! The probes stay in their slots as the camera moves: a world probe's slot
//! is its coordinates modulo the grid's size, so nothing is copied.
//!
//! They start again, warming up as when the GI is switched on, when the
//! light they gather jumps from one frame to the next ([`LightKey`]): a look
//! or lighting settings swapped, not the sun crossing the sky.

use anyhow::Result;
use voxear_core::math::{Quat, Vec3};
use voxear_rhi::{Bindless, Gpu, ImageDesc, vk};

use crate::{
    gpu::{DDGI_CASCADE_SCALE, DDGI_CASCADES, DDGI_PROBES, DDGI_RAYS, DDGI_TEXELS, Push},
    graph::{FrameGraph, ImageAccess, ImageId},
    passes::bind,
    pipelines::Pipelines,
};

const FORMAT: vk::Format = vk::Format::R16G16B16A16_SFLOAT;
const TILES_ACROSS: u32 = 128;

/// How many frames every probe traces when the GI starts, or starts again.
pub const WARM_UP_FRAMES: u32 = 8;

/// The light the probes gather, as far as it changes all at once: the sun's
/// (its colour times its strength, and the way it shines) and the sky's
/// three colours times its ambient, without lightning's flash. A probe keeps
/// most of what it held and lets go of light only as it falls, so after a
/// look or lighting settings are swapped it would hold the old light for a
/// minute and more, patch by patch as the probes come due: a night's purple
/// under a day's trees. When the light jumps the probes start again.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LightKey {
    pub sun: Vec3,
    pub sun_way: Vec3,
    pub sky: [Vec3; 3],
    pub atmosphere: bool,
}

/// How far apart, as a share of the brighter, two lights are when the light
/// has jumped; and the least light that share is taken of, so a dark night's
/// small changes count as nothing.
const JUMP_SHARE: f32 = 0.2;
const JUMP_FLOOR: f32 = 0.05;
/// The least turn of the sun, in degrees, in one frame, that is a jump.
const JUMP_TURN_DEGREES: f32 = 5.0;

impl LightKey {
    /// Whether the light jumped from `before` to this, from one frame to the
    /// next: not the sun crossing the sky or weather drifting in over
    /// seconds, which move it a little a frame.
    pub fn jumped_from(&self, before: &LightKey) -> bool {
        let apart = |a: Vec3, b: Vec3| (a - b).length() > JUMP_SHARE * a.length().max(b.length()).max(JUMP_FLOOR);
        // A sun that comes or goes is told by its light.
        let both = self.sun_way != Vec3::ZERO && before.sun_way != Vec3::ZERO;
        let turned = both && self.sun_way.dot(before.sun_way) < JUMP_TURN_DEGREES.to_radians().cos();
        self.atmosphere != before.atmosphere
            || apart(self.sun, before.sun)
            || turned
            || self.sky.iter().zip(&before.sky).any(|(a, b)| apart(*a, *b))
    }
}

/// The probes in one cascade.
const fn probe_count() -> u32 {
    DDGI_PROBES[0] * DDGI_PROBES[1] * DDGI_PROBES[2]
}

/// Each cascade's lowest probe, in its own probes from the world's origin,
/// so that the camera is in its middle; `spacing` is the finest cascade's.
pub fn origins(camera: Vec3, spacing: f32) -> [[i32; 4]; DDGI_CASCADES] {
    std::array::from_fn(|cascade| {
        let size = spacing * DDGI_CASCADE_SCALE.powi(cascade as i32);
        let middle = (camera / size).floor();
        let half = |axis: usize| (DDGI_PROBES[axis] / 2) as f32;
        [(middle.x - half(0)) as i32, (middle.y - half(1)) as i32, (middle.z - half(2)) as i32, 0]
    })
}

/// A rotation for this frame's rays, the rows of its matrix, so probes
/// sample other directions each frame.
pub fn rotation(frame: u64) -> [[f32; 4]; 3] {
    // A Weyl sequence of angles keeps successive turns far apart.
    let t = frame as f32;
    let axis =
        Vec3::new((t * 0.7548777).fract() * 2.0 - 1.0, (t * 0.5698403).fract() * 2.0 - 1.0, 0.5).normalize_or(Vec3::Y);
    let turn = Quat::from_axis_angle(axis, (t * 0.6180340).fract() * std::f32::consts::TAU);
    let m = voxear_core::math::Mat3::from_quat(turn);
    let row = |r: usize| [m.x_axis[r], m.y_axis[r], m.z_axis[r], 0.0];
    [row(0), row(1), row(2)]
}

/// The probes' images, kept from frame to frame.
#[derive(Clone, Copy)]
pub struct Probes {
    /// The rays traced this frame, a texel each: the light found and how
    /// far, negative for the back of a surface.
    pub rays: ImageId,
    pub irradiance: ImageId,
    pub depth: ImageId,
    /// Each probe's state: w 1 while it lights the scene, 0 inside a wall.
    pub states: ImageId,
    /// Its rays are traced in a compute shader through the scene's hierarchy,
    /// not by ray tracing.
    pub software: bool,
}

impl Probes {
    pub fn new(fg: &mut FrameGraph<'_>, gpu: &Gpu, bindless: &mut Bindless, software: bool) -> Result<Self> {
        let usage = vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::SAMPLED;
        let tile_rows = probe_count() / TILES_ACROSS * DDGI_CASCADES as u32;
        let maps = ImageDesc::new(TILES_ACROSS * DDGI_TEXELS, tile_rows * DDGI_TEXELS, FORMAT, usage);
        let rays = ImageDesc::new(16 * DDGI_RAYS, probe_count() / 16 * DDGI_CASCADES as u32, FORMAT, usage);
        Ok(Self {
            rays: fg.history_image(gpu, bindless, "ddgi rays", rays)?,
            irradiance: fg.history_image(gpu, bindless, "ddgi irradiance", maps)?,
            depth: fg.history_image(gpu, bindless, "ddgi depth", maps)?,
            states: fg.history_image(
                gpu,
                bindless,
                "ddgi states",
                ImageDesc::new(TILES_ACROSS, tile_rows, FORMAT, usage),
            )?,
            software,
        })
    }
}

/// Records the rays and update passes, once the scene's top level is built.
/// The rays' hits are lit by local lights through their shadows, and the
/// sun and lights through glass, by `shadow_maps`. The rays walk the view's
/// Voxel Volumes too, which `voxels` (what their making writes) holds.
pub fn record(
    fg: &mut FrameGraph<'_>,
    (pipelines, push): (Pipelines, Push),
    probes: Probes,
    shadow_maps: &[ImageId],
    voxels: &[crate::graph::BufferId],
) {
    let traced = if probes.software { Some(pipelines.ddgi_software) } else { pipelines.ddgi };
    let Some([rays_pipeline, update_pipeline]) = traced else {
        return;
    };
    let Probes { rays, irradiance, depth, states, .. } = probes;
    let groups = probe_count() * DDGI_CASCADES as u32;

    let trace = Push { output: fg.storage(rays, 0).0, ..push };
    let mut images = vec![
        (rays, ImageAccess::StorageWrite),
        (irradiance, ImageAccess::Sampled),
        (depth, ImageAccess::Sampled),
        (states, ImageAccess::Sampled),
    ];
    images.extend(shadow_maps.iter().map(|&map| (map, ImageAccess::Sampled)));
    let reads: Vec<_> = voxels.iter().map(|&pool| (pool, crate::graph::BufferAccess::ShaderRead)).collect();
    fg.add_pass("DDGI rays", &images, &reads, move |ctx| {
        bind(ctx, rays_pipeline, vk::PipelineBindPoint::COMPUTE, &trace);
        unsafe { ctx.gpu.device.cmd_dispatch(ctx.cmd, groups, 1, 1) };
    });

    let storage = |image| fg.storage(image, 0).0;
    let update = Push { textures: [storage(rays), storage(irradiance), storage(depth), storage(states)], ..push };
    let images = [
        (rays, ImageAccess::StorageWrite),
        (irradiance, ImageAccess::StorageWrite),
        (depth, ImageAccess::StorageWrite),
        (states, ImageAccess::StorageWrite),
    ];
    fg.add_pass("DDGI update", &images, &[], move |ctx| {
        bind(ctx, update_pipeline, vk::PipelineBindPoint::COMPUTE, &update);
        unsafe { ctx.gpu.device.cmd_dispatch(ctx.cmd, groups, 1, 1) };
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_grids_centre_on_the_camera_each_twice_as_wide_as_the_last() {
        let at = origins(Vec3::new(10.2, 1.7, -3.1), 1.0);
        assert_eq!(at[0], [10 - 16, 1 - 8, -4 - 16, 0]);
        assert_eq!(at[1], [5 - 16, -8, -2 - 16, 0]);
        assert_eq!(at[2], [2 - 16, -8, -1 - 16, 0]);
    }

    #[test]
    fn each_frame_turns_the_rays_a_different_way() {
        assert_ne!(rotation(1), rotation(2));
        let rows = rotation(7);
        let length = |r: [f32; 4]| (r[0] * r[0] + r[1] * r[1] + r[2] * r[2]).sqrt();
        assert!(rows.iter().all(|&r| (length(r) - 1.0).abs() < 1e-4), "a rotation keeps lengths");
    }

    /// A clear day's light: a warm sun high in the south and a blue sky.
    fn day() -> LightKey {
        LightKey {
            sun: Vec3::new(3.2, 3.0, 2.8),
            sun_way: Vec3::new(0.0, -0.8, 0.6).normalize(),
            sky: [Vec3::new(0.13, 0.3, 0.72), Vec3::new(0.6, 0.72, 0.85), Vec3::new(0.17, 0.15, 0.12)],
            atmosphere: false,
        }
    }

    #[test]
    fn the_probes_start_again_when_the_light_jumps_and_not_as_it_drifts() {
        let day = day();
        assert!(!day.jumped_from(&day), "the same light");
        // A minute's day in an hour of frames: the sun turns a tenth of a degree a frame, and dims a little.
        let later =
            LightKey { sun: day.sun * 0.995, sun_way: Quat::from_rotation_x(0.1f32.to_radians()) * day.sun_way, ..day };
        assert!(!later.jumped_from(&day), "the sun crossing the sky");
        // A night look swapped in: a dim blue moon and a dark purple sky.
        let night = LightKey {
            sun: day.sun * 0.18 * Vec3::new(0.72, 0.74, 1.0),
            sky: [Vec3::new(0.003, 0.002, 0.013), Vec3::new(0.018, 0.012, 0.055), Vec3::new(0.002, 0.001, 0.004)],
            ..day
        };
        assert!(night.jumped_from(&day) && day.jumped_from(&night), "a look swapped, either way");
        // A warmer sun of the same strength, as a golden look's.
        let golden = LightKey { sun: Vec3::new(3.2, 2.3, 1.4), ..day };
        assert!(golden.jumped_from(&day), "the sun's colour swapped");
        let turned = LightKey { sun_way: Vec3::new(0.6, -0.8, 0.0), ..day };
        assert!(turned.jumped_from(&day), "the sun put somewhere else");
        assert!(LightKey { atmosphere: true, ..day }.jumped_from(&day), "the sky's kind swapped");
        // With no sun, its way is nothing, and nothing turns.
        let sunless = LightKey { sun: Vec3::ZERO, sun_way: Vec3::ZERO, ..day };
        assert!(!sunless.jumped_from(&sunless), "no sun, still");
        assert!(sunless.jumped_from(&day), "the sun gone");
        // Light near none, as a night with no moon, counts as none, however it changes.
        let pitch = LightKey { sky: [Vec3::splat(0.004); 3], ..sunless };
        let twice = LightKey { sky: pitch.sky.map(|light| light * 2.0), ..pitch };
        assert!(!twice.jumped_from(&pitch), "near none, twice as bright");
    }
}
