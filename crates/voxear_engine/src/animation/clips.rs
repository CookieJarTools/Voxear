//! Clips as assets: a clip is a part of the model file that holds it,
//! referenced as `Models/Walks.glb#clip:Walk_Loop`, as meshes and materials
//! are. The world's Animators name the clips they play; before each tick the
//! game resolves those it has not yet (`give_clips`), and the animation code
//! reads them from [`Clips`], made to fit each character's own skeleton by
//! [`retarget`](super::retarget).

use std::{collections::HashMap, sync::Arc};

use voxear_assets::ClipImport;
use voxear_ecs::World;
use voxear_render::{Clip, ModelAnimation};

use super::Animator;
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
}

impl Clips {
    pub fn get(&self, reference: &str) -> Option<&ClipSource> {
        self.found.get(reference)?.as_ref()
    }
}

/// Resolves the clips the world's Objs play that it has not yet, and every
/// one again after a clip file or its settings changed.
pub fn give_clips(world: &mut World, content: &mut Content) {
    let wanted: Vec<String> = world
        .query_ref::<&Animator>()
        .into_iter()
        .map(|animator| animator.clip.clone())
        .filter(|clip| !clip.is_empty())
        .collect();
    if world.resource::<Clips>().is_none() {
        world.insert_resource(Clips::default());
    }
    let Some(clips) = world.resource_mut::<Clips>() else { return };
    if clips.version != content.clips_version() {
        clips.found.clear();
        clips.version = content.clips_version();
    }
    for reference in wanted {
        if clips.found.contains_key(&reference) {
            continue;
        }
        let source = content.parse(&reference).ok().and_then(|parsed| content.clip(&parsed));
        clips.found.insert(reference, source);
    }
}
