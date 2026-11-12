//! Caves made of rooms: what a Cave Network node places in the ground, and
//! where it placed them, for whatever wants to put things in a cave or
//! send a player to one.
//!
//! A room is shapes and exits as numbers ([`Room`], read by the editor
//! from a prefab). A network sets rooms in the regions of a grid and joins
//! them with tunnels (`network`): without end, a room a region
//! ([`CaveLayout::Endless`]), or as one bounded level laid out once
//! ([`CaveLayout::Level`], `level`).
//!
//! [`Caves`] gives the rooms as they lie in the generator's world:
//! [`WorldGen::caves`](super::WorldGen::caves) has it for a generator
//! whose Volume reads a Cave Network.

use std::sync::Arc;

mod grid;
mod level;
pub(crate) mod network;
mod parts;
mod rooms;

pub use network::CaveLayout;
pub use parts::{CaveExit, CaveMode, CaveShape};
pub use rooms::{Room, RoomExit, RoomForm, RoomShape, UNTURNED};

pub(crate) use network::{Network, Settings};

use grid::{Region, V3, add, dot, sub, times};
use network::Placed;

/// A room as it lies in the ground.
#[derive(Clone, Debug, PartialEq)]
pub struct PlacedRoom {
    /// Its place among a level's rooms, the main path's first: none in an
    /// endless network.
    pub index: Option<usize>,
    /// The region of the network's grid it is in.
    pub region: [i32; 3],
    /// Which of the network's rooms it is, and that room's prefab.
    pub room: usize,
    pub prefab: Option<String>,
    /// Where its origin lies, and its own x, y and z there: its prefab
    /// placed so lies as the room does.
    pub at: [f32; 3],
    pub axes: [[f32; 3]; 3],
    /// How it was turned from its prefab: mirrored along its x, then this
    /// many quarter turns about its y.
    pub mirrored: bool,
    pub turns: u8,
    /// Whether it is on a level's main path.
    pub main: bool,
    /// A box that holds it.
    pub bounds: [[f32; 3]; 2],
    /// A place on its floor with room to stand.
    pub floor: [f32; 3],
    pub exits: Vec<PlacedExit>,
    /// The rooms a tunnel joins it to: each one's region, and for a level
    /// its place among the rooms. More than its exits lead to, where two
    /// tunnels leave by one exit.
    pub joined: Vec<([i32; 3], Option<usize>)>,
}

/// One of a placed room's exits.
#[derive(Clone, Debug, PartialEq)]
pub struct PlacedExit {
    pub at: [f32; 3],
    /// The way out, one long.
    pub way: [f32; 3],
    /// The region of the room the tunnel from it leads to, and for a level
    /// that room's place among its rooms: none for an exit no tunnel uses.
    pub leads_to: Option<[i32; 3]>,
    pub leads_to_index: Option<usize>,
}

/// The cave network of a generator: its rooms, where they lie in the
/// generator's world.
#[derive(Clone, Debug)]
pub struct Caves {
    net: Arc<Network>,
}

/// How many regions a search for rooms near a place looks through at
/// most.
const SEARCHED: usize = 2048;

impl Caves {
    pub(crate) fn new(net: Arc<Network>) -> Self {
        Self { net }
    }

    pub fn layout(&self) -> CaveLayout {
        self.net.set.layout
    }

    /// The rooms of a level, the main path's first and in order, then its
    /// side rooms: none for an endless network.
    pub fn rooms(&self) -> Vec<PlacedRoom> {
        let net = &*self.net;
        let Some(level) = &net.level else { return Vec::new() };
        let lift = [0.0, net.level_height(), 0.0];
        let index = |region: Region| level.all.rooms.iter().position(|room| room.plan.region == region);
        let rooms = level.all.rooms.iter().enumerate();
        rooms.map(|(at, room)| shown(net, room, lift, Some(at), at < level.main, &index)).collect()
    }

    /// The rooms whose origin lies within `reach` metres of `at`, nearest
    /// first: of a level, or of an endless network.
    pub fn rooms_near(&self, at: [f32; 3], reach: f32) -> Vec<PlacedRoom> {
        let net = &*self.net;
        let apart = |room: &PlacedRoom| {
            let away = sub(room.at, at);
            dot(away, away)
        };
        let mut rooms = match &net.level {
            Some(_) => self.rooms(),
            None => {
                let grid = net.grid();
                // Outward from the region `at` is in, a region at a time, while they are near enough.
                let (foot, _) = grid.foot(at);
                let near = |region: Region| {
                    let away = sub(grid.stand(region, [0.0, 0.0]).foot, foot);
                    dot(away, away).sqrt() <= reach + net.set.spacing * 1.5
                };
                let mut regions = vec![grid.region(at)];
                let mut looked = 0;
                while looked < regions.len() && regions.len() < SEARCHED {
                    for next in grid.beside(regions[looked]) {
                        if !regions.contains(&next) && near(next) {
                            regions.push(next);
                        }
                    }
                    looked += 1;
                }
                regions.into_iter().filter_map(|region| self.room(region)).collect()
            }
        };
        rooms.retain(|room| apart(room) <= reach * reach);
        rooms.sort_by(|a, b| apart(a).total_cmp(&apart(b)));
        rooms
    }

    /// The room of `region`: none where it has none.
    pub fn room(&self, region: [i32; 3]) -> Option<PlacedRoom> {
        let net = &*self.net;
        if net.level.is_some() {
            return self.rooms().into_iter().find(|room| room.region == region);
        }
        let grid = net.grid();
        let plan = net.plan(&grid, region)?;
        let ground = net.grounds(&[plan.stand.foot])[0];
        let rise = -net.set.depth * (0.75 + 0.5 * plan.deep);
        let room = net.placed(&grid, plan, ground, rise);
        Some(shown(net, &room, [0.0; 3], None, false, &|_| None))
    }

    /// The room whose hollow `at` is in: none in a tunnel or in rock.
    pub fn room_at(&self, at: [f32; 3]) -> Option<PlacedRoom> {
        let net = &*self.net;
        let rooms = self.rooms_near(at, net.set.reach() + net.set.spacing);
        rooms.into_iter().find(|room| {
            let from = sub(at, room.at);
            let own = [dot(from, room.axes[0]), dot(from, room.axes[1]), dot(from, room.axes[2])];
            net.set.rooms[room.room].inside(own, u64::MAX, None) > 0.0
        })
    }
}

/// `room` as others see it, moved by `lift`.
fn shown(
    net: &Network,
    room: &Placed,
    lift: V3,
    index: Option<usize>,
    main: bool,
    index_of: &dyn Fn(Region) -> Option<usize>,
) -> PlacedRoom {
    let own = &net.set.rooms[room.plan.room];
    let at = add(room.at, lift);
    let out = |v: V3| add(add(times(room.axes[0], v[0]), times(room.axes[1], v[1])), times(room.axes[2], v[2]));
    let exits = own.exits.iter().enumerate().map(|(exit, placed)| {
        let slot = (0..4).find(|&slot| room.plan.links[slot] && room.plan.toward[slot] == Some(exit));
        let leads_to = slot.map(|slot| room.plan.beside[slot]);
        PlacedExit {
            at: add(at, out(placed.at)),
            way: out(placed.way),
            leads_to,
            leads_to_index: leads_to.and_then(index_of),
        }
    });
    // Its floor: down from the first place over its origin that is well inside it, to where its hollow ends.
    let inside = |v: V3| own.inside(v, room.plan.present, None);
    let reach = own.reach();
    let over =
        (1..).map(|step| step as f32 * 0.5).take_while(|&up| up <= reach).find(|&up| inside([0.0, up, 0.0]) > 0.4);
    let floor = over.map_or([0.0; 3], |over| {
        let mut down = over;
        while down > -reach && inside([0.0, down - 0.25, 0.0]) > 0.0 {
            down -= 0.25;
        }
        [0.0, down, 0.0]
    });
    PlacedRoom {
        index,
        region: room.plan.region,
        room: room.plan.room,
        prefab: own.prefab.clone(),
        at,
        axes: room.axes,
        mirrored: room.plan.mirrored,
        turns: room.plan.turns,
        main,
        bounds: [add(room.bounds.low, lift), add(room.bounds.high, lift)],
        floor: add(at, out(floor)),
        exits: exits.collect(),
        joined: (0..4)
            .filter(|&slot| room.plan.links[slot])
            .map(|slot| (room.plan.beside[slot], index_of(room.plan.beside[slot])))
            .collect(),
    }
}
