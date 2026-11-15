//! Ray-traced audio: the game's sound heard through the scene the GPU's
//! rays already trace for the GI, its models, the chunks of its worlds of
//! blocks and its terrain, rather than through the colliders alone. With
//! it on, each frame asks the renderer (see `voxear_render::SoundRays`)
//! for:
//!
//! - **occlusion**: for each 3D Audio Source with occlusion on, rays from
//!   the listener across its sphere, which give what reaches the listener
//!   past the walls in each band of pitch, as Steam Audio's occlusion and
//!   transmission do;
//! - **reverb**: rays from the listener bounced round the room, whose
//!   energy field, added up over a few frames, gives how long the room
//!   rings and its first echoes (see `voxear_audio::sound_field`).
//!
//! What comes back is a few frames old, about 15 milliseconds at 144
//! frames a second, which sound does not hear. The rays a frame follow
//! the project's quality and its budget of GPU time: the pass's time is
//! read back each frame, and the rays are cut while the least of the last
//! 60 frames' is over the budget, and let grow again while it is well
//! under. A source with pathing on, in a level with baked paths, is still
//! heard round the walls by Steam Audio, through the colliders.
//!
//! Without it, or with no GPU to trace on, sound is heard as before:
//! through the colliders by Steam Audio, and with the level's baked
//! reverb probes, where it has them, for its reverb (see
//! [`crate::reverb_probes`]).

use std::collections::{HashMap, VecDeque};

use serde::{Deserialize, Serialize};
use voxear_audio::{
    ReverbTimes,
    sound_field::{EarlyReflection, EnergyField, FIELD_SECONDS, RayQuality},
    steam_audio::{AcousticMaterial, Muffle},
};
use voxear_core::math::Vec3;
use voxear_ecs::Entity;
use voxear_physics::scene::Acoustic;
use voxear_render::{
    MAX_SOUND_SOURCES, MAX_SOUND_SPOTS, SOUND_FIXED, SoundHeard, SoundMaterial, SoundRays, SoundSource,
};

/// The name of the renderer's pass, whose GPU time the budget is held to.
pub const PASS: &str = "Sound rays";
/// How many frames' times the budget looks back over.
const COST_FRAMES: usize = 60;
/// The fewest rays from the listener a frame, however tight the budget.
const FEWEST_RAYS: u32 = 64;
/// How far the listener may move, in metres, before the field added up
/// so far is let go of faster.
const MOVED: f32 = 2.0;
/// Rays and bounces for a reverb probe's bake, which takes its time.
const BAKE_RAYS: u32 = 8192;
const BAKE_BOUNCES: u32 = 16;
/// How often the reverb's times and early echoes are given to the mixer, in
/// frames.
const ROOM_EVERY: u32 = 4;

/// Ray-traced audio for the whole project, in `voxear.toml`'s `[audio]`, or
/// for a level, in its `[audio]`, over the project's. Off by default.
///
/// ```toml
/// [audio]
/// ray_traced = true
/// ray_quality = "high"
/// ray_budget = 0.3
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SoundTracing {
    pub on: bool,
    pub quality: RayQuality,
    /// The most GPU time the sound rays may take a frame, in milliseconds.
    pub budget: f32,
}

impl Default for SoundTracing {
    fn default() -> Self {
        Self { on: false, quality: RayQuality::Medium, budget: DEFAULT_BUDGET }
    }
}

/// The GPU time sound rays take at most a frame unless a project says:
/// 0.3 ms, a twentieth of a frame at 144 frames a second.
pub const DEFAULT_BUDGET: f32 = 0.3;

/// A [`RayQuality`] as `voxear.toml`, a level and a player's settings write it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RayQualityName {
    Low,
    #[default]
    Medium,
    High,
}

impl From<RayQualityName> for RayQuality {
    fn from(name: RayQualityName) -> Self {
        match name {
            RayQualityName::Low => Self::Low,
            RayQualityName::Medium => Self::Medium,
            RayQualityName::High => Self::High,
        }
    }
}

impl From<RayQuality> for RayQualityName {
    fn from(quality: RayQuality) -> Self {
        match quality {
            RayQuality::Low => Self::Low,
            RayQuality::Medium => Self::Medium,
            RayQuality::High => Self::High,
        }
    }
}

/// A level's own ray-traced audio, in its `.level` file's `[audio]`: each
/// left out takes the project's.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LevelAudio {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ray_traced: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ray_quality: Option<RayQualityName>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ray_budget: Option<f32>,
}

impl LevelAudio {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// The project's settings with this level's laid over them.
    pub fn over(&self, project: SoundTracing) -> SoundTracing {
        SoundTracing {
            on: self.ray_traced.unwrap_or(project.on),
            quality: self.ray_quality.map_or(project.quality, RayQuality::from),
            budget: self.ray_budget.unwrap_or(project.budget),
        }
    }
}

/// The acoustic presets, as the sound rays take them, in [`Acoustic`]'s order.
pub fn sound_materials() -> Vec<SoundMaterial> {
    Acoustic::ALL
        .iter()
        .map(|&acoustic| {
            let AcousticMaterial { absorption, scattering, transmission } = crate::acoustics::material(acoustic);
            SoundMaterial { absorption, scattering, transmission }
        })
        .collect()
}

/// A source asked of this frame: its key, where it is, its sphere's
/// radius, its rays, and whether walls let some of it through.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Traced {
    pub entity: Entity,
    pub at: Vec3,
    pub radius: f32,
    pub volumetric: bool,
    pub transmission: bool,
}

/// A reverb probe bake on its way: the spots still to trace, and the
/// energy fields of those traced, in order.
struct Bake {
    spots: Vec<Vec3>,
    asked: usize,
    fields: Vec<Option<EnergyField>>,
}

/// The game's side of the sound rays, which `GameAudio` keeps.
pub struct SoundTracer {
    /// The project's settings, and the level's over them.
    pub project: SoundTracing,
    pub level: LevelAudio,
    /// A script's own, over both, until the game stops.
    pub script: Option<SoundTracing>,
    /// A renderer took the last request: rays can be traced.
    renderer: bool,
    next_id: u64,
    /// Each request in flight: its id, its sources' Objs and whether each
    /// lets some through, and how many bake spots it carries, from where.
    in_flight: VecDeque<(u64, Vec<(Entity, bool, u32)>, Option<(usize, usize)>, bool)>,
    /// What reaches the listener of each source, as last traced.
    muffles: HashMap<Entity, Muffle>,
    /// The listener's energy field, added up, where it was last traced
    /// from, and how many frames are in it.
    field: EnergyField,
    field_at: Option<Vec3>,
    frames: u32,
    /// The rays from the listener a frame, as the budget allows.
    rays: u32,
    /// The pass's last frames' GPU times, in milliseconds.
    costs: VecDeque<f32>,
    /// The settings under which even the fewest rays took more than the
    /// budget, so that sound is heard without rays until they change.
    over_budget: Option<SoundTracing>,
    /// This frame's listener and sources, and whether the room is wanted.
    listener: Option<Vec3>,
    sources: Vec<Traced>,
    room: bool,
    room_frame: u32,
    bake: Option<Bake>,
}

impl Default for SoundTracer {
    fn default() -> Self {
        Self {
            project: SoundTracing::default(),
            level: LevelAudio::default(),
            script: None,
            renderer: false,
            next_id: 1,
            in_flight: VecDeque::new(),
            muffles: HashMap::new(),
            field: EnergyField::default(),
            field_at: None,
            frames: 0,
            rays: RayQuality::Medium.rays(),
            costs: VecDeque::new(),
            over_budget: None,
            listener: None,
            sources: Vec::new(),
            room: false,
            room_frame: 0,
            bake: None,
        }
    }
}

impl SoundTracer {
    /// The settings in force: a script's, or the level's over the project's.
    pub fn settings(&self) -> SoundTracing {
        self.script.unwrap_or_else(|| self.level.over(self.project))
    }

    /// Whether sound is heard by rays now: asked for, with a renderer to
    /// trace them.
    pub fn tracing(&self) -> bool {
        let settings = self.settings();
        settings.on && self.renderer && self.over_budget != Some(settings)
    }

    /// Whether the GPU could not trace even the fewest rays within the
    /// budget, so that sound goes without until the settings change.
    pub fn over_budget(&self) -> bool {
        self.over_budget == Some(self.settings())
    }

    /// How many rays from the listener a frame it traces now.
    pub fn rays(&self) -> u32 {
        self.rays
    }

    /// The least GPU time the pass took over the last 60 frames, in
    /// milliseconds; none before it has run.
    pub fn cost(&self) -> Option<f32> {
        self.costs.iter().copied().reduce(f32::min)
    }

    /// Starts this frame's asking: the listener, where the game hears from.
    pub(crate) fn begin(&mut self, listener: Option<Vec3>) {
        self.listener = listener;
        self.sources.clear();
        self.room = false;
    }

    /// Asks for a source's occlusion this frame, and gives what was last
    /// traced of it: in the open until a frame has come back.
    pub(crate) fn hear(&mut self, source: Traced) -> Muffle {
        if self.sources.len() < MAX_SOUND_SOURCES {
            self.sources.push(source);
        }
        self.muffles.get(&source.entity).copied().unwrap_or(Muffle::NONE)
    }

    /// Asks for the room this frame, and gives how it rings and its first
    /// echoes every few frames, once a frame has come back; none between.
    pub(crate) fn room(&mut self) -> Option<(ReverbTimes, Vec<EarlyReflection>)> {
        self.room = true;
        self.room_frame = self.room_frame.wrapping_add(1);
        if self.frames == 0 || self.room_frame % ROOM_EVERY != 0 {
            return None;
        }
        Some((self.field.reverb_times(), self.field.early_reflections()))
    }

    /// Lets go of what was traced, as when the game stops.
    pub(crate) fn forget(&mut self) {
        self.muffles.clear();
        self.field = EnergyField::default();
        self.field_at = None;
        self.frames = 0;
        self.script = None;
    }

    /// Starts a bake of reverb probes at these spots, in place of any on
    /// its way.
    pub fn bake(&mut self, spots: Vec<Vec3>) {
        let fields = vec![None; spots.len()];
        self.bake = Some(Bake { spots, asked: 0, fields });
    }

    /// How far the bake on its way has come: spots traced, and of how many.
    pub fn baking(&self) -> Option<(usize, usize)> {
        let bake = self.bake.as_ref()?;
        Some((bake.fields.iter().filter(|field| field.is_some()).count(), bake.spots.len()))
    }

    /// The finished bake, once: each spot's reverb times, in order. A
    /// spot inside something, which hears nothing back, has none.
    pub fn take_bake(&mut self) -> Option<Vec<Option<ReverbTimes>>> {
        let done = self.bake.as_ref()?.fields.iter().all(Option::is_some);
        if !done {
            return None;
        }
        let bake = self.bake.take()?;
        Some(
            bake.fields
                .into_iter()
                .map(|field| field.filter(|field| field.total() > 1e-6).map(|field| field.reverb_times()))
                .collect(),
        )
    }

    /// Once a frame, after the game's sound: takes what the renderer found
    /// and asks it for this frame's rays, or none.
    pub fn exchange(&mut self, renderer: Option<&mut voxear_render::Renderer>) {
        let Some(renderer) = renderer else {
            self.renderer = false;
            return;
        };
        self.renderer = true;
        if let Some(heard) = renderer.sound_heard() {
            self.take(heard);
        }
        // A bake's frames trace far more than the game's, and say nothing of what the game's cost.
        let timed = renderer.timings().iter().find(|(name, _)| name == PASS).filter(|_| self.bake.is_none());
        if let Some(&(_, ms)) = timed {
            self.costs.push_back(ms);
            while self.costs.len() > COST_FRAMES {
                self.costs.pop_front();
            }
        }
        let request = self.request();
        renderer.trace_sound(request);
    }

    /// This frame's request: the listener's sources and room while tracing,
    /// and the next of a bake's spots.
    fn request(&mut self) -> Option<SoundRays> {
        let settings = self.settings();
        let tracing = self.tracing() && self.listener.is_some();
        let bake_spots = self.bake.as_ref().map(|bake| {
            let from = bake.asked.min(bake.spots.len());
            let room = MAX_SOUND_SPOTS - usize::from(tracing && self.room);
            (from, (bake.spots.len() - from).min(room))
        });
        let baking = bake_spots.is_some_and(|(_, count)| count > 0);
        if !tracing && !baking {
            // A bake's last spots may still be on their way back.
            if self.bake.is_none() {
                self.in_flight.clear();
            }
            return None;
        }
        self.budget(settings);
        let id = self.next_id;
        self.next_id += 1;
        let listener = self.listener.unwrap_or_default();
        let mut spots = Vec::new();
        let traces_room = tracing && self.room;
        if traces_room {
            spots.push(listener);
        }
        if let (Some(bake), Some((from, count))) = (&mut self.bake, bake_spots) {
            spots.extend_from_slice(&bake.spots[from..from + count]);
            bake.asked = from + count;
        }
        let occlusion = settings.quality.occlusion_rays();
        let sources: Vec<_> = if tracing { self.sources.clone() } else { Vec::new() };
        let asked = SoundRays {
            id,
            listener,
            sources: sources
                .iter()
                .map(|s| SoundSource { at: s.at, radius: s.radius, rays: if s.volumetric { occlusion } else { 1 } })
                .collect(),
            spots,
            rays: if baking { BAKE_RAYS } else { self.rays },
            bounces: if baking { BAKE_BOUNCES } else { settings.quality.bounces() },
            seconds: FIELD_SECONDS,
            materials: sound_materials(),
        };
        let keys =
            sources.iter().map(|s| (s.entity, s.transmission, if s.volumetric { occlusion } else { 1 })).collect();
        self.in_flight.push_back((id, keys, bake_spots.filter(|_| baking), traces_room));
        while self.in_flight.len() > 8 {
            self.in_flight.pop_front();
        }
        Some(asked)
    }

    /// Cuts the rays while the pass's least time of late is over the
    /// budget, and lets them grow back while it is well under, to the
    /// quality's most.
    fn budget(&mut self, settings: SoundTracing) {
        let most = settings.quality.rays();
        let Some(least) = self.cost().filter(|_| self.costs.len() >= 8) else {
            self.rays = self.rays.clamp(FEWEST_RAYS, most);
            return;
        };
        if least > settings.budget && self.rays == FEWEST_RAYS && self.costs.len() >= COST_FRAMES {
            // The fewest rays take longer than the budget, as tracing in software on a slow GPU may.
            tracing::info!(
                target: "audio",
                "Sound rays take {least:.2} ms at their fewest, over the {:.2} ms budget: rooms ring by their baked \
                 reverb probes instead",
                settings.budget
            );
            self.over_budget = Some(settings);
            self.costs.clear();
        } else if least > settings.budget && self.rays > FEWEST_RAYS {
            self.rays = ((self.rays as f32 * 0.8) as u32).max(FEWEST_RAYS);
            // Times taken with more rays no longer say what these cost.
            self.costs.clear();
        } else if least < settings.budget * 0.5 && self.rays < most {
            self.rays = ((self.rays as f32 * 1.1) as u32 + 1).min(most);
        }
        self.rays = self.rays.clamp(FEWEST_RAYS, most);
    }

    fn take(&mut self, heard: SoundHeard) {
        let Some(at) = self.in_flight.iter().position(|(id, ..)| *id == heard.id) else { return };
        let (_, keys, bake, room) = self.in_flight.remove(at).expect("found");
        // Requests answered later than this one never come.
        self.in_flight.drain(..at);
        for ((entity, transmission, rays), (open, through)) in keys.into_iter().zip(&heard.sources) {
            let open = (*open as f32 / rays.max(1) as f32).clamp(0.0, 1.0);
            let blocked = (rays as f32 - open * rays as f32).max(1.0);
            let muffle = match transmission {
                true => Muffle(through.map(|t| open + (1.0 - open) * (t / blocked).clamp(0.0, 1.0))),
                false => Muffle([open; 3]),
            };
            self.muffles.insert(entity, muffle);
        }
        let mut fields = heard.fields.iter();
        if room && let Some(numbers) = fields.next() {
            let field = EnergyField::from_fixed(numbers, SOUND_FIXED);
            let quality = self.settings().quality;
            let moved = match (self.field_at, self.listener) {
                (Some(was), Some(now)) => was.distance(now) > MOVED,
                _ => true,
            };
            // The first frame, or a listener far from where the field was added up, starts it again.
            let keep = if self.frames == 0 || moved { 0.0 } else { quality.keep() };
            self.field.blend(&field, keep);
            self.frames = self.frames.saturating_add(1);
            if moved || self.field_at.is_none() {
                self.field_at = self.listener;
            }
        }
        if let (Some((from, _)), Some(bake)) = (bake, self.bake.as_mut()) {
            for (index, numbers) in fields.enumerate() {
                if let Some(slot) = bake.fields.get_mut(from + index) {
                    *slot = Some(EnergyField::from_fixed(numbers, SOUND_FIXED));
                }
            }
        }
        // Spots asked of in a request that never came back are asked of again.
        if let Some(bake) = self.bake.as_mut()
            && self.in_flight.iter().all(|(.., spots, _)| spots.is_none())
        {
            bake.asked = bake.fields.iter().position(Option::is_none).unwrap_or(bake.spots.len());
        }
    }

    /// Forgets the occlusion of sources no longer asked of.
    pub(crate) fn keep_only(&mut self, alive: &dyn Fn(Entity) -> bool) {
        self.muffles.retain(|entity, _| alive(*entity));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_levels_settings_lie_over_the_projects() {
        let project = SoundTracing { on: true, quality: RayQuality::Low, budget: 0.5 };
        let level = LevelAudio { ray_quality: Some(RayQualityName::High), ..LevelAudio::default() };
        assert_eq!(level.over(project), SoundTracing { quality: RayQuality::High, ..project });
        let off: LevelAudio = toml::from_str("ray_traced = false").unwrap();
        assert!(!off.over(project).on);
        assert!(LevelAudio::default().is_empty());
    }

    #[test]
    fn the_rays_follow_the_budget() {
        let mut tracer = SoundTracer::default();
        let settings = SoundTracing { on: true, quality: RayQuality::High, budget: 0.3 };
        tracer.rays = 4096;
        tracer.costs.extend([0.6; 10]);
        tracer.budget(settings);
        assert!(tracer.rays < 4096 && tracer.costs.is_empty(), "over budget: {}", tracer.rays);
        let cut = tracer.rays;
        tracer.costs.extend([0.05; 10]);
        tracer.budget(settings);
        assert!(tracer.rays > cut, "well under: {}", tracer.rays);
        tracer.costs.clear();
        tracer.costs.extend([0.2; 10]);
        let held = tracer.rays;
        tracer.budget(settings);
        assert_eq!(tracer.rays, held, "within it");
    }

    #[test]
    fn the_fewest_rays_over_budget_give_way_to_the_baked_probes_until_the_settings_change() {
        let mut tracer = SoundTracer { renderer: true, ..SoundTracer::default() };
        tracer.project = SoundTracing { on: true, quality: RayQuality::Low, budget: 0.1 };
        tracer.rays = FEWEST_RAYS;
        tracer.costs.extend([0.7; COST_FRAMES]);
        tracer.budget(tracer.settings());
        assert!(!tracer.tracing() && tracer.over_budget(), "given up");
        tracer.script = Some(SoundTracing { budget: 1.0, ..tracer.project });
        assert!(tracer.tracing(), "a larger budget tries again");
    }

    #[test]
    fn a_bakes_spots_go_a_few_a_frame_and_come_back_in_order() {
        let mut tracer = SoundTracer::default();
        let spots: Vec<Vec3> = (0..12).map(|i| Vec3::new(i as f32, 0.0, 0.0)).collect();
        tracer.bake(spots);
        let first = tracer.request().expect("a bake asks without the game running");
        assert_eq!(first.spots.len(), MAX_SOUND_SPOTS);
        let second = tracer.request().expect("the rest");
        assert_eq!(second.spots.len(), 3);
        assert!(tracer.request().is_none(), "nothing left to ask");
        let answer = |rays: &SoundRays, energy: i32| SoundHeard {
            id: rays.id,
            sources: Vec::new(),
            fields: rays.spots.iter().map(|_| vec![energy; voxear_render::SOUND_FIELD]).collect(),
            rays: rays.rays,
        };
        tracer.take(answer(&first, 1 << 20));
        assert_eq!(tracer.baking(), Some((9, 12)));
        assert!(tracer.take_bake().is_none(), "not yet");
        tracer.take(answer(&second, 0));
        let baked = tracer.take_bake().expect("done");
        assert_eq!(baked.len(), 12);
        assert!(baked[..9].iter().all(Option::is_some) && baked[9..].iter().all(Option::is_none), "{baked:?}");
    }
}
