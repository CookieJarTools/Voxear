//! Light through Portals: an open Portal whose far side is bright lights
//! what stands in front of it, as a window onto a sunny field does, or a
//! door into a fire.
//!
//! Each Portal that lights is a rectangle light over its opening, shining
//! out of its front. What it shines is what its far side shows: each time
//! the camera's own view draws that far side, a pass reads the opening at
//! 16 by 16 points across it, where nothing stands in front of it, into a
//! picture of its own kept from frame to frame, and writes their average
//! for the CPU to read a frame or two later. [`PortalLight::Average`]
//! shines that average, one colour; [`PortalLight::Picture`] shines the
//! picture itself, each part of the opening its own colour, so the sky's
//! blue falls one way and a fire's red the other. Until a far side has been
//! seen, its Portal sheds no light; once seen, it keeps the last light it
//! was seen with while the camera looks elsewhere. A closed Portal's light
//! fades out in a few frames.

use std::collections::HashMap;

use voxear_core::math::Vec3;
use voxear_ecs::{Entity, World};
use voxear_rhi::{Buffer, FRAMES_IN_FLIGHT, Gpu, MemoryLocation, vk};

use voxear_scene::{Transform, WorldTransform};

use crate::{
    gpu::{GpuLight, LIGHT_RECTANGLE, NO_SHADOW, NO_TEXTURE},
    portals::{Portal, PortalLight, PortalPose, PortalQuality},
    scene::ExtractedFrame,
    shadow_cache::ShadowLight,
    shadows::LocalLight,
};

/// The most Portals that light at once, the nearest the camera first.
pub const MOST: usize = 4;
/// How many points a side a Portal's picture reads its opening at.
pub const PICTURE: u32 = 16;
/// How much of the way to a new measure its light goes each time it is
/// read, so a flicker on the far side does not flicker the light.
const TOWARD: f32 = 0.35;
/// How much of its light a closed Portal keeps each frame.
const FADING: f32 = 0.8;

/// What a Portal's light last was, as its far side was seen.
#[derive(Clone, Copy, Debug, Default)]
struct Glow {
    /// Linear radiance, what its opening shows on average.
    colour: Vec3,
    /// Whether its far side was ever seen to measure.
    measured: bool,
    /// How much of its light it sheds now: all while it is open, less each
    /// frame once it is closed, and back again as it opens.
    open: f32,
    /// The frame it last lit in.
    lit: u64,
}

/// A Portal near enough to light this frame: the light it is among the
/// frame's, once its far side has been seen to measure. Its far side is
/// measured whenever the view draws it, lit yet or not.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Lit {
    pub entity: Entity,
    pub light: Option<usize>,
}

/// Each view's Portals' lights, kept from frame to frame.
#[derive(Default)]
pub(crate) struct PortalGlows {
    glows: HashMap<Entity, Glow>,
    /// For each frame in flight, where its measures are written and whose
    /// they are.
    readbacks: [Option<(Buffer, Vec<Entity>)>; FRAMES_IN_FLIGHT],
}

impl PortalGlows {
    /// Takes the measures frame `slot` wrote when it last ran, now done:
    /// each seen far side's average, eased towards.
    pub fn take(&mut self, slot: usize) {
        let Some((buffer, entities)) = self.readbacks[slot].as_mut() else { return };
        let values: &[[f32; 4]] = bytemuck::cast_slice(&buffer.read()[..entities.len() * 16]);
        for (entity, value) in entities.iter().zip(values) {
            let [r, g, b, seen] = *value;
            let colour = Vec3::new(r, g, b);
            if seen <= 0.02 || !colour.is_finite() {
                continue;
            }
            let glow = self.glows.entry(*entity).or_default();
            glow.colour = if glow.measured { glow.colour.lerp(colour, TOWARD) } else { colour };
            glow.measured = true;
        }
        entities.clear();
    }

    /// Adds a rectangle light over the opening of each open Portal that
    /// lights, within its view distance of the camera, the nearest
    /// [`MOST`] of them, each casting shadows while there is room; and
    /// lets closed ones fade.
    pub fn place(
        &mut self,
        world: &World,
        extracted: &mut ExtractedFrame,
        quality: &PortalQuality,
        frame: u64,
    ) -> Vec<Lit> {
        if quality.light == PortalLight::Off {
            self.glows.clear();
            return Vec::new();
        }
        let camera = Vec3::from(extracted.frame.camera_position);
        let mut near: Vec<(f32, Entity, Portal, PortalPose)> = world
            .query_ref::<(Entity, &Portal, &Transform, Option<&WorldTransform>)>()
            .into_iter()
            .filter(|(entity, portal, ..)| {
                portal.to.is_some()
                    && portal.light > 0.0
                    && voxear_scene::dimension_of(world, *entity) == extracted.dimension
            })
            .map(|(entity, portal, local, placed)| {
                let pose = PortalPose::of(local, placed);
                (pose.position.distance(camera), entity, *portal, pose)
            })
            .filter(|(distance, _, portal, _)| *distance <= portal.reach(quality))
            .collect();
        near.sort_by(|a, b| a.0.total_cmp(&b.0));
        near.truncate(MOST);
        let mut lit = Vec::new();
        for (_, entity, portal, pose) in near {
            let glow = self.glows.entry(entity).or_default();
            glow.open = if portal.open { (glow.open + 1.0 - FADING).min(1.0) } else { glow.open * FADING };
            glow.lit = frame;
            if !glow.measured || glow.open < 0.01 || glow.colour.max_element() < 1e-4 {
                lit.push(Lit { entity, light: None });
                continue;
            }
            // The picture is the light's colour, all of it; or one colour, its average.
            let shown = if quality.light == PortalLight::Picture { Vec3::ONE } else { glow.colour } * glow.open;
            let half = [portal.size.x * 0.5, portal.size.y * 0.5];
            let area = portal.size.x * portal.size.y;
            // Just out of its front, so the opening's own frame does not shade it.
            let middle = pose.position + pose.rotation * Vec3::new(0.0, half[1], 0.0) + pose.front() * 0.02;
            let range = (area.sqrt() * 4.0).clamp(6.0, 20.0);
            let light = GpuLight {
                position: middle.to_array(),
                range,
                color: (shown * area * portal.light).to_array(),
                kind: LIGHT_RECTANGLE,
                direction: pose.front().to_array(),
                cos_outer: 0.0,
                right: (pose.rotation * Vec3::X).to_array(),
                cos_inner: 0.0,
                size: half,
                cookie: NO_TEXTURE,
                profile: NO_TEXTURE,
                shadow: NO_SHADOW,
                shadow_near: 0.0,
                shadow_far: 0.0,
                shadow_radius: half[0].max(half[1]),
            };
            let index = extracted.lights.len();
            extracted.lights.push(light);
            if extracted.shadow_lights.len() < crate::gpu::LOCAL_SHADOW_LIGHTS {
                let extent = Vec3::new(half[0], half[1], 0.0).length();
                let shadow = LocalLight {
                    position: middle,
                    rotation: pose.rotation,
                    range: range + extent,
                    near: (extent * 1.1).max(0.1),
                    cone_degrees: None,
                };
                extracted.shadow_lights.push(ShadowLight { index, entity, light: shadow });
            }
            lit.push(Lit { entity, light: Some(index) });
        }
        // Those let go a while ago are forgotten.
        self.glows.retain(|_, glow| glow.lit + 600 > frame);
        extracted.frame.light_count = extracted.lights.len() as u32;
        lit
    }

    /// Forgets what Portal `entity`'s far side was last seen to show, as
    /// when its picture is made again.
    pub fn forget(&mut self, entity: Entity) {
        if let Some(glow) = self.glows.get_mut(&entity) {
            glow.measured = false;
        }
    }

    /// Where frame `slot` writes the measures of the far sides it draws,
    /// made the first time, and notes which Portals they are, in turn.
    pub fn measures(&mut self, gpu: &Gpu, slot: usize, entities: Vec<Entity>) -> anyhow::Result<u64> {
        if self.readbacks[slot].is_none() {
            let usage = vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS;
            let buffer = gpu.create_buffer(
                &format!("portal light {slot}"),
                (MOST * 16) as u64,
                usage,
                MemoryLocation::GpuToCpu,
            )?;
            self.readbacks[slot] = Some((buffer, Vec::new()));
        }
        let (buffer, kept) = self.readbacks[slot].as_mut().expect("made above");
        *kept = entities;
        Ok(buffer.address)
    }

    /// Its buffers, to let go once no frame in flight uses them.
    pub fn take_buffers(&mut self) -> Vec<Buffer> {
        self.readbacks.iter_mut().filter_map(Option::take).map(|(buffer, _)| buffer).collect()
    }
}
