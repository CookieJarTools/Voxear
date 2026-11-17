//! Navigation in a level: each world of blocks' places, kept up with its
//! blocks (see `voxear_nav`).
//!
//! A world's places are made once something asks for them, as a Block Mob
//! does by walking in it, and from then on follow its blocks as they
//! stream in, stream out and change: on navigation's own threads, handed
//! over whole. A small world's first places are made at once; a world
//! without end makes its own in the background as its ground comes.

use std::{collections::HashMap, sync::Arc};

use voxear_ecs::{Entity, World};
use voxear_nav::{BlockNav, BlockNavBuilder, blocks::walking::WALKERS};
use voxear_terrain::blocks::BlockWorld;

/// Every world of blocks' navigation, by its Obj, for those something has
/// asked for.
#[derive(Default)]
pub struct NavWorlds {
    worlds: HashMap<Entity, BlockNavBuilder>,
}

impl NavWorlds {
    /// World `entity`'s navigation as last made, if it is kept.
    pub fn nav(&self, entity: Entity) -> Option<&Arc<BlockNav>> {
        self.worlds.get(&entity).map(BlockNavBuilder::nav)
    }

    /// Each world kept: its Obj and its builder.
    pub fn worlds(&self) -> impl Iterator<Item = (Entity, &BlockNavBuilder)> {
        self.worlds.iter().map(|(entity, builder)| (*entity, builder))
    }
}

/// Keeps every world's navigation up with its blocks, and lets go of
/// those whose worlds are gone. Cheap where nothing changed: a look at two
/// pointers a world.
pub fn update(world: &mut World) {
    let Some(mut navs) = world.remove_resource::<NavWorlds>() else { return };
    voxear_core::profile_scope!("Navigation");
    navs.worlds.retain(|entity, builder| match world.get::<BlockWorld>(*entity) {
        Some(blocks) => {
            builder.update(&blocks.blocks, &blocks.faces);
            true
        }
        None => false,
    });
    world.insert_resource(navs);
}

/// World `entity`'s navigation, kept from now on: made now where it is
/// small, or begun in the background where it is not.
pub fn nav_of(world: &mut World, entity: Entity) -> Option<Arc<BlockNav>> {
    let blocks = world.get::<BlockWorld>(entity)?;
    let (blocks, faces) = (blocks.blocks.clone(), blocks.faces.clone());
    if world.resource::<NavWorlds>().is_none() {
        world.insert_resource(NavWorlds::default());
    }
    let navs = world.resource_mut::<NavWorlds>()?;
    let builder = navs.worlds.entry(entity).or_insert_with(|| {
        let mut builder = BlockNavBuilder::new(&WALKERS);
        builder.update(&blocks, &faces);
        builder
    });
    Some(builder.nav().clone())
}

/// Waits until every kept world's navigation is made from its blocks as
/// they are now: before play begins, and in tests.
pub fn settle(world: &mut World) {
    let Some(mut navs) = world.remove_resource::<NavWorlds>() else { return };
    for (entity, builder) in &mut navs.worlds {
        if let Some(blocks) = world.get::<BlockWorld>(*entity) {
            builder.settle(&blocks.blocks, &blocks.faces);
        }
    }
    world.insert_resource(navs);
}
