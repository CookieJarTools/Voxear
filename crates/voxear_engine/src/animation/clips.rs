//! Clips as assets: a clip is a part of the model file that holds it,
//! referenced as `Models/Walks.glb#clip:Walk_Loop`, as meshes and materials
//! are. The world's Animators name the clips they play, and its Motion
//! Matchers the Motion Sets they play, and gun actions the clips they are
//! based on; before each tick the game resolves
//! those it has not yet (`give_clips`), and the animation code reads them
//! from [`Clips`], made to fit each character's own skeleton by
//! [`retarget`](super::retarget).

use std::{collections::HashMap, sync::Arc};

use voxear_assets::ClipImport;
use voxear_ecs::World;
use voxear_render::{Clip, ModelAnimation};

use super::{Animator, MotionMatcher, MotionSet};
use crate::content::Content;

/// What a clip reference's part starts with.
pub const CLIP_PART: &str = "clip:";

/// A clip, from whichever model file holds it, and how its settings there
/// say it moves and loops.
#[derive(Clone)]
pub struct ClipSource {
    /// The file's skeleton, skins and clips.
    pub animation: Arc<ModelAnimation>,
    pub index: usize,
    pub settings: ClipImport,
}

impl ClipSource {
    pub fn clip(&self) -> &Clip {
        &self.animation.clips[self.index]
    }
}

/// Every clip the world's Objs play, by reference, as the game resolved
/// them; none for one that could not be, as a file without that clip.
#[derive(Default)]
pub struct Clips {
    /// The content's clips' version these were resolved at: when a file or
    /// its clip settings change, they are resolved again.
    version: u64,
    found: HashMap<String, Option<ClipSource>>,
    /// Motion Sets, by reference.
    sets: HashMap<String, Option<Arc<MotionSet>>>,
}

impl Clips {
    pub fn get(&self, reference: &str) -> Option<&ClipSource> {
        self.found.get(reference)?.as_ref()
    }

    /// Whether a clip by this reference has been looked for, found or not.
    pub fn tried(&self, reference: &str) -> bool {
        self.found.contains_key(reference)
    }

    pub fn set(&self, reference: &str) -> Option<&Arc<MotionSet>> {
        self.sets.get(reference)?.as_ref()
    }

    /// Gives a clip by a reference without a file, as tests do with clips
    /// they make: it stays until the content's clips change.
    pub fn put(&mut self, reference: &str, source: ClipSource) {
        self.found.insert(reference.to_string(), Some(source));
    }

    /// Goes up as the content's clips' version does: what is made from
    /// clips is made again when it changes.
    pub fn version(&self) -> u64 {
        self.version
    }
}

/// Whether a clip scripts asked about before it was found has been looked
/// for since, found or not, as each tick does in the editor too: so what
/// read it while it was being found, as a drawer's `ClipFrame`, can read it
/// again. Each such clip says so once.
pub fn clips_found_for_scripts(world: &mut World) -> bool {
    let Some(waiting) = world.resource::<super::pose_ops::WaitingClips>() else { return false };
    let clips = world.resource::<Clips>();
    let looked: Vec<String> = waiting.0.iter().filter(|r| clips.is_some_and(|c| c.tried(r))).cloned().collect();
    let Some(waiting) = world.resource_mut::<super::pose_ops::WaitingClips>() else { return false };
    for reference in &looked {
        waiting.0.remove(reference);
    }
    !looked.is_empty()
}

/// Resolves the clips and Motion Sets the world's Objs play that it has
/// not yet, and every one again after a clip file, its settings or a set
/// changed.
pub fn give_clips(world: &mut World, content: &mut Content) {
    let mut wanted: Vec<String> = world
        .query_ref::<&Animator>()
        .into_iter()
        .map(|animator| animator.clip.clone())
        .filter(|clip| !clip.is_empty())
        .collect();
    let sets: Vec<String> = world
        .query_ref::<&MotionMatcher>()
        .into_iter()
        .map(|matcher| matcher.motion_set.clone())
        .filter(|set| !set.is_empty())
        .collect();
    wanted.extend(crate::guns::handling::clips_wanted(world));
    if let Some(asked) = world.resource::<super::pose_ops::ScriptClips>() {
        wanted.extend(asked.0.iter().cloned());
    }
    if world.resource::<Clips>().is_none() {
        world.insert_resource(Clips::default());
    }
    let Some(clips) = world.resource_mut::<Clips>() else { return };
    if clips.version != content.clips_version() {
        clips.found.clear();
        clips.sets.clear();
        clips.version = content.clips_version();
    }
    for reference in sets {
        let set = clips.sets.entry(reference.clone()).or_insert_with(|| {
            let guid = content.parse(&reference).ok().and_then(|parsed| parsed.guid());
            guid.and_then(|guid| content.motion_set(guid))
        });
        wanted.extend(set.iter().flat_map(|set| set.clips()).map(str::to_string));
    }
    for reference in wanted {
        if clips.found.contains_key(&reference) {
            continue;
        }
        let source = content.parse(&reference).ok().and_then(|parsed| content.clip(&parsed));
        clips.found.insert(reference, source);
    }
}
