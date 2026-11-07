//! A Motion Set's state made to play on a character: its loops and the
//! clip it stands with, each fitted to the character's skeleton, a loop
//! played backwards reversed, and the database a Motion Matcher searches
//! over them. Made once per set, state and character, and again whenever a
//! clip file, its settings or the set change.

use std::sync::{Arc, Weak};

use voxear_assets::{ClipImport, ClipMotion};
use voxear_ecs::World;
use voxear_render::{Clip, ModelAnimation};

use super::{
    clips::Clips,
    motion_database::{MotionDatabase, database_for},
    motion_set::MotionSet,
    retarget::fitted,
};

/// A state ready to play: its clips on the character's skeleton, the
/// database over them, the one it stands with among them, and how long
/// changing into it takes to settle.
pub struct StatePlay {
    pub animation: Arc<ModelAnimation>,
    pub database: Arc<MotionDatabase>,
    pub idle: Option<usize>,
    pub blend: f32,
}

impl StatePlay {
    /// A model's own clips as one state, for a Motion Matcher with no set:
    /// standing with the first loop of them that stands still.
    pub fn own(animation: Arc<ModelAnimation>, database: Arc<MotionDatabase>) -> Self {
        let idle = database.runs.iter().find(|run| run.stands()).map(|run| run.clip);
        Self { animation, database, idle, blend: 0.25 }
    }
}

/// Made states, kept while their character's model is.
#[derive(Default)]
struct StatePlays {
    /// The clips' version they were made at.
    version: u64,
    made: Vec<(String, usize, Weak<ModelAnimation>, Arc<StatePlay>)>,
}

/// State `state` of the set at `reference` made to play on `target`; with
/// no set, the model's own clips. `None` when it has nothing to play.
pub fn state_play(
    world: &mut World,
    reference: &str,
    set: Option<&MotionSet>,
    state: usize,
    target: &Arc<ModelAnimation>,
) -> Option<Arc<StatePlay>> {
    let Some(set) = set else {
        let database = database_for(world, target)?;
        return Some(Arc::new(StatePlay::own(target.clone(), database)));
    };
    let version = world.resource::<Clips>().map_or(0, Clips::version);
    if world.resource::<StatePlays>().is_none() {
        world.insert_resource(StatePlays::default());
    }
    let plays = world.resource_mut::<StatePlays>()?;
    if plays.version != version {
        plays.made.clear();
        plays.version = version;
    }
    plays.made.retain(|(_, _, model, _)| model.strong_count() > 0);
    let found = plays
        .made
        .iter()
        .find(|(r, s, model, _)| r == reference && *s == state && Weak::ptr_eq(model, &Arc::downgrade(target)));
    if let Some((.., play)) = found {
        return Some(play.clone());
    }
    let play = Arc::new(make(world, set, state, target)?);
    let plays = world.resource_mut::<StatePlays>()?;
    plays.made.push((reference.to_string(), state, Arc::downgrade(target), play.clone()));
    Some(play)
}

/// Fits a state's clips to `target` and builds the database over them.
fn make(world: &mut World, set: &MotionSet, state: usize, target: &Arc<ModelAnimation>) -> Option<StatePlay> {
    let state = set.states.get(state)?;
    let mut clips: Vec<Clip> = Vec::new();
    let mut settings: Vec<ClipImport> = Vec::new();
    let source =
        |world: &World, reference: &str| world.resource::<Clips>().and_then(|clips| clips.get(reference)).cloned();
    for entry in &state.clips {
        let Some(found) = source(world, &entry.clip) else { continue };
        let clip = fitted(world, &found, target);
        let mut setting = found.settings;
        clips.push(match entry.backwards {
            true => {
                // Backwards, a clip set to move goes the other way.
                if setting.motion == ClipMotion::Set {
                    setting.direction += 180.0;
                }
                reversed(&clip)
            }
            false => (*clip).clone(),
        });
        settings.push(setting);
    }
    let idle = match source(world, &state.idle) {
        Some(found) => {
            clips.push((*fitted(world, &found, target)).clone());
            settings.push(found.settings);
            Some(clips.len() - 1)
        }
        None => None,
    };
    let animation = Arc::new(ModelAnimation { skeleton: target.skeleton.clone(), skins: Vec::new(), clips });
    let database = Arc::new(MotionDatabase::build(&animation, &settings)?);
    let idle = idle.or_else(|| database.runs.iter().find(|run| run.stands()).map(|run| run.clip));
    Some(StatePlay { animation, database, idle, blend: state.blend })
}

/// A clip played from its end to its start.
fn reversed(clip: &Clip) -> Clip {
    let bones = clip.poses.len() / clip.frames.max(1);
    let poses = (0..clip.frames).rev().flat_map(|frame| clip.frame(frame).iter().copied()).collect();
    debug_assert_eq!(bones * clip.frames, clip.poses.len());
    Clip { name: format!("{} (backwards)", clip.name), frames: clip.frames, poses }
}
