//! Cave rooms, from the prefabs they are made as to the numbers a world
//! generator keeps.
//!
//! A Cave Network node lists its rooms as prefabs. Making ground reads no
//! prefab: the node keeps each room's shapes and exits as numbers, in the
//! generator's file, and these are what a game and every player joining it
//! read. [`room_of`] reads a prefab's Cave Shape and Cave Exit Objs into
//! those numbers, and [`read_rooms`] does so for every room a graph lists.
//! The editor calls them when a room is picked and whenever a prefab
//! changes, so the numbers are worked out once, where the room is made.

use anyhow::{Context as _, Result, bail};
use voxear_core::math::{EulerRot, Mat4, Quat, Vec3};
use voxear_ecs::{Entity, World};
use voxear_scene::{Destroying, Transform, WorldTransform};
use voxear_terrain::{
    blocks::BlockWorld,
    generate::{CaveLayout, CaveMode, Caves, Graph, PlacedRoom, Room, RoomExit, RoomShape},
};

use crate::{Content, blocks::BlockWorldAsset, prefab};

/// A number as a room keeps it: to a tenth of a millimetre, so a file's
/// numbers are short and reading a prefab twice gives the same.
fn kept(value: f32) -> f32 {
    let rounded = (value * 10_000.0).round() / 10_000.0;
    // Nothing is written as nothing, whichever side of it it fell.
    if rounded == 0.0 { 0.0 } else { rounded }
}

/// The shapes and exits of the room that is prefab `reference`, in the
/// space of the prefab's root, in the order of its Objs.
pub fn room_of(content: &mut Content, reference: &str) -> Result<(Vec<RoomShape>, Vec<RoomExit>)> {
    let guid = content.parse(reference)?.guid().with_context(|| format!("`{reference}` is not a prefab"))?;
    let objs = prefab::expand_prefab(content, guid)?;
    // Where each Obj lies in the root's space: the root itself at its middle, unturned.
    let mut placed: Vec<Mat4> = Vec::with_capacity(objs.len());
    for (at, obj) in objs.iter().enumerate() {
        let Some(parent) = &obj.parent else {
            placed.push(Mat4::IDENTITY);
            continue;
        };
        let [x, y, z] = obj.data.rotation().map(f32::to_radians);
        let own = Mat4::from_scale_rotation_translation(
            Vec3::from(obj.data.scale()),
            Quat::from_euler(EulerRot::YXZ, y, x, z),
            Vec3::from(obj.data.position()),
        );
        // A parent comes before its children in a prefab's Objs; one that does not is taken as the root.
        let over = objs[..at].iter().position(|other| other.path == *parent).map_or(Mat4::IDENTITY, |at| placed[at]);
        placed.push(over * own);
    }
    let (mut shapes, mut exits) = (Vec::new(), Vec::new());
    for (obj, placed) in objs.iter().zip(&placed) {
        let at = placed.w_axis.truncate();
        let axes = [placed.x_axis.truncate(), placed.y_axis.truncate(), placed.z_axis.truncate()];
        if let Some(shape) = obj.data.cave_shape {
            let size = [0, 1, 2].map(|axis| kept((shape.size[axis] * axes[axis].length()).abs()));
            let turned = axes.map(|axis| axis.normalize_or_zero());
            let turn = [0, 1, 2].map(|axis| turned[axis].to_array().map(kept));
            shapes.push(RoomShape {
                shape: shape.shape,
                fill: shape.mode == CaveMode::Fill,
                chance: kept(shape.chance.clamp(0.0, 1.0)),
                margin: kept(shape.margin.max(0.0)),
                at: at.to_array().map(kept),
                size,
                turn: [turn[0], turn[1], turn[2]]
                    .concat()
                    .try_into()
                    .unwrap_or(voxear_terrain::generate::caves::UNTURNED),
            });
        }
        if obj.data.cave_exit.is_some() {
            let way = axes[2].normalize_or(Vec3::Z);
            exits.push(RoomExit { at: at.to_array().map(kept), way: way.to_array().map(kept) });
        }
    }
    Ok((shapes, exits))
}

/// `graph` with every room its Cave Networks list read again from its
/// prefab: none where nothing changed. A room whose prefab cannot be read
/// keeps the numbers it had.
pub fn read_rooms(content: &mut Content, graph: &Graph) -> Option<Graph> {
    let mut read = graph.clone();
    for node in &mut read.nodes {
        let Some(rooms) = node.kind.rooms_mut() else { continue };
        for room in rooms {
            read_room(content, room);
        }
    }
    (read != *graph).then_some(read)
}

/// Reads `room`'s shapes and exits from its prefab again: whether they
/// changed.
pub fn read_room(content: &mut Content, room: &mut Room) -> bool {
    let Some(reference) = &room.prefab else { return false };
    match room_of(content, reference) {
        Ok((shapes, exits)) => {
            let changed = shapes != room.shapes || exits != room.exits;
            (room.shapes, room.exits) = (shapes, exits);
            changed
        }
        Err(error) => {
            tracing::warn!("a cave room's prefab {reference} could not be read: {error:#}");
            false
        }
    }
}

/// Whether any Cave Network of `graph` lists the prefab `guid`.
pub fn lists_room(content: &Content, graph: &Graph, guid: voxear_assets::AssetGuid) -> bool {
    let rooms = graph.nodes.iter().filter_map(|node| node.kind.rooms()).flatten();
    rooms
        .filter_map(|room| room.prefab.as_deref())
        .any(|reference| content.parse(reference).ok().and_then(|reference| reference.guid()) == Some(guid))
}

/// The caves of the world of blocks on `entity`: its generator's, the
/// radius of its planet where it is one, and how far its generator's world
/// is from the level's.
fn caves_of(world: &World, content: &mut Content, entity: Entity) -> Option<(Caves, Option<f32>, Vec3)> {
    let asset = world.get::<BlockWorldAsset>(entity)?;
    let generate = asset.file.generate.as_ref()?;
    let (planet, [x, z]) = (generate.planet.map(|planet| planet.radius), generate.position);
    let caves = crate::blocks::generator(content, &asset.file)?.caves()?;
    let corner = voxear_terrain::corner(world.get::<Transform>(entity)?, world.get::<WorldTransform>(entity));
    // A planet's middle is its corner; flat ground starts `position` into its generator's world.
    let apart = if planet.is_some() { corner } else { corner - Vec3::new(x, 0.0, z) };
    Some((caves, planet, apart))
}

/// The cave rooms of a world of blocks as JSON, for scripts (see
/// `ScriptServices::caves_json`, which says what `what` and `at` ask for).
pub fn caves_json(world: &World, content: &mut Content, obj: u64, what: i32, at: [f32; 4]) -> Result<String> {
    let found = match obj {
        // The first world of blocks with caves, in the order of their Objs.
        0 => {
            let mut worlds: Vec<Entity> = world
                .query_ref::<(Entity, &BlockWorld)>()
                .without::<Destroying>()
                .into_iter()
                .map(|(e, _)| e)
                .collect();
            worlds.sort_by_key(|entity| entity.to_bits());
            worlds.into_iter().find_map(|entity| Some((entity, caves_of(world, content, entity)?)))
        }
        obj => {
            let entity = Entity::from_bits(obj);
            caves_of(world, content, entity).map(|caves| (entity, caves))
        }
    };
    let Some((entity, (caves, planet, apart))) = found else { bail!("no world of blocks with a cave network") };
    let place = (Vec3::new(at[0], at[1], at[2]) - apart).to_array();
    let rooms: Vec<PlacedRoom> = match what {
        0 => caves.rooms(planet),
        1 => caves.rooms_near(planet, place, at[3].max(0.0)),
        2 => caves.room_at(planet, place).into_iter().collect(),
        3 => caves.room(planet, [at[0] as i32, at[1] as i32, at[2] as i32]).into_iter().collect(),
        4 => Vec::new(),
        other => bail!("caves asked for {other}, which is nothing"),
    };
    let moved = |p: [f32; 3]| (Vec3::from(p) + apart).to_array();
    let rooms: Vec<serde_json::Value> = rooms
        .iter()
        .map(|room| {
            let exits: Vec<serde_json::Value> = room
                .exits
                .iter()
                .map(|exit| {
                    serde_json::json!({
                        "at": moved(exit.at), "way": exit.way, "to": exit.leads_to, "toIndex": exit.leads_to_index,
                    })
                })
                .collect();
            let joined: Vec<serde_json::Value> = room
                .joined
                .iter()
                .map(|(region, index)| serde_json::json!({ "to": region, "toIndex": index }))
                .collect();
            serde_json::json!({
                "joined": joined,
                "index": room.index, "region": room.region, "room": room.room, "prefab": room.prefab,
                "at": moved(room.at), "x": room.axes[0], "y": room.axes[1], "z": room.axes[2],
                "mirrored": room.mirrored, "turns": room.turns, "main": room.main,
                "low": moved(room.bounds[0]), "high": moved(room.bounds[1]), "floor": moved(room.floor),
                "exits": exits,
            })
        })
        .collect();
    let layout = match caves.layout() {
        CaveLayout::Endless => "endless",
        CaveLayout::Level => "level",
    };
    Ok(serde_json::json!({ "obj": entity.to_bits(), "layout": layout, "rooms": rooms }).to_string())
}
