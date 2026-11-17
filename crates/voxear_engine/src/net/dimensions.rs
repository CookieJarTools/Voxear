//! Dimensions in a session: which each player has, which Objs go to them,
//! and a dimension's edits sent as they come into it.
//!
//! **On the server**, each player has the dimension their own Obj is in and
//! the one the Portal open for them leads into (see `portals::open_for`):
//! the server makes the ground round each of those, and they are what the
//! player's machine is told to make. Each is named to that player once, by
//! what it is ([`DimensionWhat`]: a `.dimension` asset and its key, or a
//! dungeon's gate), and is a small number on that connection from then on;
//! the level's is always 0 and never named. When one stops being theirs,
//! the player is told they may let it go. An Obj goes to a player only
//! while it is in a dimension they have, its range measured within it.
//! Every player has the level's, as every machine makes it.
//!
//! As a dimension is named to a player, its edits follow: every change of
//! its worlds of blocks from how they are made, in the form saves keep them
//! (`VXBLOCKS`), cut into pieces, those nearest where the player comes into
//! it first, sent within a share of the connection's budget, then a word
//! that they are all sent. The player's machine takes them in and says it
//! is ready; until then the server takes none of the player's Objs through
//! a Portal into it, and their machine keeps its Portals into it closed.
//! The level's edits go the same way as a player joins. Changes made as the
//! game plays go as the game's own RPCs, as before.
//!
//! **Crossings.** The server decides where its Objs are, and tells each
//! player that has an Obj when it goes into another dimension, for the tick
//! it went, so it is shown going there as it reaches the Portal. A player's
//! own Obj that their machine runs (shared authority) goes through Portals
//! there; their machine tells the server, which checks there is a Portal by
//! it into that dimension and moves it, or tells it where it is instead.

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

use voxear_core::math::Vec3;
use voxear_ecs::{Entity, World};
use voxear_net::Channel;
use voxear_scene::DimensionId;

use super::messages::{DimensionWhat, Message};
use super::replication::{NetId, Replication, SpawnRequest};
use super::{End, NetAuthority, NetSession, Networked, PlayerId};

/// The share of a connection's budget a dimension's edits may take while
/// they are sent, the rest left for the game.
const EDIT_SHARE: f64 = 0.5;
/// How far from a Portal into a dimension, or from the one it leads to, a
/// player's Obj that their machine says went through may be, in metres,
/// for the server to take its word.
const CROSSING_REACH: f32 = 8.0;
/// How much further than the dimension distance a Portal into a dimension
/// already made may be from a player for them to have that dimension, in
/// metres.
const PORTAL_KEPT_PAST: f32 = 6.0;
/// Ticks past a crossing's on this machine after which it is shown where
/// no snapshot says the server's world has reached it: more than a client is
/// ahead of its server and shows behind it.
const SHOWN_WITHOUT_SNAPSHOTS: u64 = 16;
/// Ticks a player who joined waits for an Obj of their own before the
/// level's edits are sent anyway, from the level's middle out.
const LEVEL_WAIT: u32 = 60;

/// One piece of a dimension's edits, for one of its worlds of blocks.
#[derive(Clone, Debug, PartialEq)]
pub struct EditPiece {
    /// The world's place among the dimension's worlds of blocks (see
    /// `Game`'s `dimension_worlds`).
    pub world: u16,
    /// Blocks the server numbered as it ran, by number, GUID and state.
    pub numbered: Vec<(u16, String, u8)>,
    /// The changes, as `BlockEdits::to_bytes` writes them.
    pub bytes: Vec<u8>,
}

/// Edits that came to a player's machine for a dimension it has made.
#[derive(Clone, Debug)]
pub struct ArrivedEdits {
    pub dimension: DimensionId,
    pub piece: EditPiece,
}

/// What the server keeps of one player's dimensions.
#[derive(Default)]
pub(super) struct Seen {
    /// Each dimension named to them, by its number on their connection and
    /// what it was named as.
    numbers: HashMap<DimensionId, (u16, DimensionWhat)>,
    next: u16,
    /// Those they have now besides the level's, which every player has.
    pub kept: HashSet<DimensionId>,
    /// Those whose edits they took in.
    pub ready: HashSet<DimensionId>,
    /// Each dimension's edits still to send, nearest first, its word that
    /// they are sent last.
    sending: VecDeque<(DimensionId, VecDeque<Message>)>,
    /// Bytes of edits they may be sent now.
    allowance: f64,
    /// Ticks since they joined with no Obj of their own, and whether the
    /// level's edits were asked for.
    waited: u32,
    level_asked: bool,
}

impl Seen {
    /// Their number for a dimension: 0 for the level's.
    pub fn number(&self, dimension: DimensionId) -> Option<u16> {
        match dimension.is_level() {
            true => Some(0),
            false => self.numbers.get(&dimension).map(|(number, _)| *number),
        }
    }

    fn dimension(&self, number: u16) -> Option<DimensionId> {
        match number {
            0 => Some(DimensionId::LEVEL),
            _ => self.numbers.iter().find(|(_, (n, _))| *n == number).map(|(id, _)| *id),
        }
    }

    /// Whether they have a dimension: the level's, or one kept.
    pub fn has(&self, dimension: DimensionId) -> bool {
        dimension.is_level() || self.kept.contains(&dimension)
    }
}

/// A dimension a player's machine was named.
struct Held {
    what: DimensionWhat,
    /// This machine's own for it, once made.
    local: Option<DimensionId>,
    /// The server has the player keep it.
    kept: bool,
    /// Its edits are all in.
    edits_in: bool,
    /// It could not be made here.
    failed: bool,
}

/// What comes to a player's machine of a dimension's edits, in order.
enum Incoming {
    Edits(u16, EditPiece),
    Done(u16),
}

/// The session's dimensions: on a server, what each is; on a client, what
/// it was named and what came for them.
#[derive(Default)]
pub(super) struct Dims {
    /// The server's dimensions, as the game describes them.
    described: HashMap<DimensionId, DimensionWhat>,
    /// Edits the game is asked for: each player's, the dimension, and where
    /// the player comes into it, for the nearest to go first.
    wanted: Vec<(PlayerId, DimensionId, Vec3)>,
    /// Players asking for their Objs to go into another dimension, when
    /// their machines run their own.
    claims: Vec<(PlayerId, NetId, u16)>,
    /// Bytes of edits sent, for the stats.
    pub edit_bytes: u64,
    // The client's.
    held: BTreeMap<u16, Held>,
    incoming: VecDeque<Incoming>,
    /// Objs to make in dimensions not made here yet, by number.
    waiting: Vec<(u16, SpawnRequest)>,
    /// Crossings of Objs not made yet, or into dimensions not made yet.
    late_crossings: Vec<(NetId, u16)>,
}

impl Dims {
    /// This machine's dimension for a number the server gave.
    pub fn local(&self, number: u16) -> Option<DimensionId> {
        match number {
            0 => Some(DimensionId::LEVEL),
            _ => self.held.get(&number).and_then(|held| held.local),
        }
    }

    /// The server's number for one of this machine's dimensions.
    fn number_of(&self, local: DimensionId) -> Option<u16> {
        match local.is_level() {
            true => Some(0),
            false => self.held.iter().find(|(_, held)| held.local == Some(local)).map(|(number, _)| *number),
        }
    }

    /// Takes in what the server said of dimensions, on a client: true where
    /// the message was one of them.
    pub fn take_in(&mut self, message: &Message, replication: &mut Replication) -> bool {
        match message {
            Message::Dimension { number, what } => {
                let held = self.held.entry(*number).or_insert_with(|| Held {
                    what: what.clone(),
                    local: None,
                    kept: true,
                    edits_in: false,
                    failed: false,
                });
                (held.kept, held.edits_in, held.failed) = (true, false, false);
            }
            Message::LetGo(number) => {
                if let Some(held) = self.held.get_mut(number) {
                    held.kept = false;
                }
            }
            Message::Edits { dimension, world, numbered, bytes } => {
                let piece = EditPiece { world: *world, numbered: numbered.clone(), bytes: bytes.clone() };
                self.incoming.push_back(Incoming::Edits(*dimension, piece));
            }
            Message::EditsDone(number) => self.incoming.push_back(Incoming::Done(*number)),
            Message::Crossed { id, dimension, tick } => match replication.remote.get_mut(id) {
                Some(remote) => {
                    remote.dimension = *dimension;
                    remote.crossings.push_back((*tick, *dimension));
                }
                None => self.late_crossings.push((*id, *dimension)),
            },
            _ => return false,
        }
        true
    }
}

/// Where each player is, on a server or a host, in their dimension: their
/// first owned Obj. Ground is made round each, and Portals near them open.
pub(crate) fn player_places(world: &World) -> Vec<(PlayerId, Vec3, DimensionId)> {
    match world.resource::<NetSession>() {
        Some(session) => places_of(world, session),
        None => Vec::new(),
    }
}

/// [`player_places`] of a session taken out of the world.
fn places_of(world: &World, session: &NetSession) -> Vec<(PlayerId, Vec3, DimensionId)> {
    if !matches!(session.end, End::Server(_)) {
        return Vec::new();
    }
    session
        .players
        .iter()
        .filter_map(|&player| {
            let entity = session.replication.player_obj(player).filter(|e| world.is_alive(*e))?;
            let place = voxear_scene::world_matrix(world, entity).w_axis.truncate();
            Some((player, place, voxear_scene::dimension_of(world, entity)))
        })
        .collect()
}

/// The dimensions the session needs kept made: on a server, those players
/// are in or have a Portal open into; on a client, those the server has it
/// keep.
pub(crate) fn needed(world: &World) -> Vec<DimensionId> {
    let Some(session) = world.resource::<NetSession>() else { return Vec::new() };
    match session.end {
        End::Server(_) => {
            let mut needed: Vec<DimensionId> = session
                .replication
                .viewers
                .values()
                .flat_map(|viewer| viewer.dimensions.kept.iter().copied())
                .collect();
            for (player, _, dimension) in player_places(world) {
                needed.push(dimension);
                needed.extend(open_into(world, player));
            }
            needed.sort_by_key(|id| id.0);
            needed.dedup();
            needed
        }
        End::Client(_) => session.dims.held.values().filter(|held| held.kept).filter_map(|held| held.local).collect(),
    }
}

/// The dimension the Portal open for a player leads into, where it leads
/// somewhere.
fn open_into(world: &World, player: PlayerId) -> Option<DimensionId> {
    let portal = crate::portals::open_for(world, player)?;
    let to = world.get::<voxear_render::Portal>(portal)?.to.filter(|to| world.is_alive(*to))?;
    Some(voxear_scene::dimension_of(world, to))
}

/// Tells the session what one of the server's dimensions is, for players'
/// machines to find or make theirs: the game describes every dimension it
/// has made each frame. One it does not describe goes to no player.
pub fn describe(world: &mut World, dimension: DimensionId, what: DimensionWhat) {
    if let Some(session) = world.resource_mut::<NetSession>() {
        session.dims.described.insert(dimension, what);
    }
}

/// Forgets what the server's dimensions were, before the game describes
/// those it has now.
pub fn forget_described(world: &mut World) {
    if let Some(session) = world.resource_mut::<NetSession>() {
        session.dims.described.clear();
    }
}

/// The edits the game is asked for on a server: each player's, the
/// dimension, and where the player comes into it.
pub fn take_edits_wanted(world: &mut World) -> Vec<(PlayerId, DimensionId, Vec3)> {
    world.resource_mut::<NetSession>().map(|s| std::mem::take(&mut s.dims.wanted)).unwrap_or_default()
}

/// Gives a player a dimension's edits to send, nearest first: then a word
/// that they are all sent.
pub fn owe_edits(world: &mut World, player: PlayerId, dimension: DimensionId, pieces: Vec<EditPiece>) {
    let Some(session) = world.resource_mut::<NetSession>() else { return };
    let Some(viewer) = session.replication.viewers.get_mut(&player) else { return };
    let seen = &mut viewer.dimensions;
    let Some(number) = seen.number(dimension).filter(|_| seen.has(dimension)) else { return };
    seen.sending.retain(|(owed, _)| *owed != dimension);
    let mut messages: VecDeque<Message> = pieces
        .into_iter()
        .map(|piece| Message::Edits {
            dimension: number,
            world: piece.world,
            numbered: piece.numbered,
            bytes: piece.bytes,
        })
        .collect();
    messages.push_back(Message::EditsDone(number));
    seen.sending.push_back((dimension, messages));
}

/// What each player has now, on the server, each with where they come into
/// it: the dimension their own Obj is in, and the one the Portal open for
/// them leads into, coming in at its far Portal.
pub(super) fn wanted_by_players(world: &World, session: &NetSession) -> HashMap<PlayerId, Vec<(DimensionId, Vec3)>> {
    let mut wanted: HashMap<PlayerId, Vec<(DimensionId, Vec3)>> = HashMap::new();
    let quality = world.resource::<voxear_render::portals::PortalQuality>().copied().unwrap_or_default();
    // Every open Portal into another dimension, where it stands and the dimension and place it leads to.
    let ways: Vec<(Entity, bool, DimensionId, Vec3, DimensionId, Vec3)> = world
        .query_ref::<(Entity, &voxear_render::Portal)>()
        .without::<voxear_scene::Destroying>()
        .into_iter()
        .filter(|(_, portal)| portal.open)
        .filter_map(|(entity, portal)| {
            let to = portal.to.filter(|to| world.is_alive(*to))?;
            let (here, far) = (voxear_scene::dimension_of(world, entity), voxear_scene::dimension_of(world, to));
            let place = |obj: Entity| voxear_scene::world_matrix(world, obj).w_axis.truncate();
            // One into a dimension loaded for it counts only while it is the one open for them.
            let loaded =
                world.has::<crate::portals::PortalInto>(entity) || world.has::<crate::dungeons::DungeonGate>(entity);
            (here != far).then(|| (entity, loaded, here, place(entity), far, place(to)))
        })
        .collect();
    for (player, place, dimension) in places_of(world, session) {
        let mut theirs = vec![(dimension, place)];
        // The one open for them into a dimension loaded for it, and any other into one there already near them.
        let open = crate::portals::open_for(world, player);
        let near = quality.dimension_distance + PORTAL_KEPT_PAST;
        for &(entity, loaded, here, at, far, far_at) in &ways {
            let by = Some(entity) == open || !loaded && here == dimension && at.distance(place) <= near;
            if by && !theirs.iter().any(|(kept, _)| *kept == far) {
                theirs.push((far, far_at));
            }
        }
        wanted.insert(player, theirs);
    }
    for player in &session.players {
        wanted.entry(*player).or_default();
    }
    wanted
}

impl Dims {
    /// Brings what a player has up to `now` (from [`wanted_by_players`]):
    /// names the dimensions new to them, asking the game for their edits,
    /// and lets go those they no longer need; the messages to send them.
    pub(super) fn update_viewer(
        &mut self,
        player: PlayerId,
        seen: &mut Seen,
        now: &[(DimensionId, Vec3)],
        has_obj: bool,
    ) -> Vec<Message> {
        let mut out = Vec::new();
        // A dimension let go on the server, or made again as something else, is named afresh.
        let stale: Vec<DimensionId> = seen
            .numbers
            .iter()
            .filter(|(id, (_, what))| self.described.get(id) != Some(what))
            .map(|(id, _)| *id)
            .collect();
        for id in stale {
            if let Some((number, _)) = seen.numbers.remove(&id)
                && seen.kept.remove(&id)
            {
                out.push(Message::LetGo(number));
            }
            seen.ready.remove(&id);
            seen.sending.retain(|(owed, _)| *owed != id);
        }
        // The level's is every player's, its edits sent once they have an Obj of their own, or after a while.
        if !seen.level_asked {
            seen.waited += 1;
            let toward = now.iter().find(|(id, _)| id.is_level()).map(|(_, at)| *at);
            if has_obj && toward.is_some() || seen.waited > LEVEL_WAIT {
                seen.level_asked = true;
                self.wanted.push((player, DimensionId::LEVEL, toward.unwrap_or(Vec3::ZERO)));
            }
        }
        for &(id, at) in now.iter().filter(|(id, _)| !id.is_level()) {
            if seen.kept.contains(&id) {
                continue;
            }
            let Some(what) = self.described.get(&id).cloned() else { continue };
            let number = match seen.numbers.get(&id) {
                Some((number, _)) => *number,
                None => {
                    seen.next = seen.next.max(1);
                    let number = seen.next;
                    seen.next = seen.next.wrapping_add(1);
                    seen.numbers.insert(id, (number, what.clone()));
                    number
                }
            };
            out.push(Message::Dimension { number, what });
            seen.kept.insert(id);
            seen.ready.remove(&id);
            self.wanted.push((player, id, at));
        }
        let going: Vec<DimensionId> =
            seen.kept.iter().copied().filter(|id| !id.is_level() && !now.iter().any(|(kept, _)| kept == id)).collect();
        for id in going {
            seen.kept.remove(&id);
            seen.ready.remove(&id);
            seen.sending.retain(|(owed, _)| *owed != id);
            if let Some(number) = seen.number(id) {
                out.push(Message::LetGo(number));
            }
        }
        out
    }
}

/// The edits a player may be sent this tick, within their share of the
/// budget: whole pieces, nearest first, each dimension's word that they are
/// all sent after its last.
pub(super) fn edits_to_send(seen: &mut Seen, budget: f64, tick_rate: u32, edit_bytes: &mut u64) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    if seen.sending.is_empty() {
        seen.allowance = 0.0;
        return out;
    }
    seen.allowance = (seen.allowance + budget * EDIT_SHARE / f64::from(tick_rate.max(1))).min(budget);
    while seen.allowance > 0.0 {
        let Some((_, messages)) = seen.sending.front_mut() else { break };
        let Some(message) = messages.pop_front() else {
            seen.sending.pop_front();
            continue;
        };
        let bytes = message.encode(voxear_core::math::I64Vec3::ZERO);
        seen.allowance -= bytes.len() as f64;
        *edit_bytes += bytes.len() as u64;
        out.push(bytes);
    }
    if seen.sending.front().is_some_and(|(_, messages)| messages.is_empty()) {
        seen.sending.pop_front();
    }
    out
}

/// Takes in, on the server, what a player said of their dimensions.
pub(super) fn heard(session: &mut NetSession, player: PlayerId, message: Message) {
    match message {
        Message::Ready(number) => {
            if let Some(viewer) = session.replication.viewers.get_mut(&player)
                && let Some(id) = viewer.dimensions.dimension(number)
                && viewer.dimensions.has(id)
            {
                viewer.dimensions.ready.insert(id);
            }
        }
        Message::Crossed { id, dimension, .. } if session.settings.authority == NetAuthority::Shared => {
            session.dims.claims.push((player, id, dimension));
        }
        _ => {}
    }
}

/// Checks, on the server, each crossing players' machines said their own
/// Objs made: one by a Portal into that dimension is made here too; any
/// other is put back, its player told where it is.
pub(super) fn check_crossings(world: &mut World, session: &mut NetSession) {
    for (player, id, number) in std::mem::take(&mut session.dims.claims) {
        let Some(tracked) = session.replication.objects.get(&id).filter(|t| t.owner == Some(player)) else { continue };
        let entity = tracked.entity;
        let Some(viewer) = session.replication.viewers.get(&player) else { continue };
        let Some(into) = viewer.dimensions.dimension(number) else { continue };
        let here = voxear_scene::dimension_of(world, entity);
        if here == into {
            continue;
        }
        let place = voxear_scene::world_matrix(world, entity).w_axis.truncate();
        let by_portal = world
            .query_ref::<(Entity, &voxear_render::Portal)>()
            .without::<voxear_scene::Destroying>()
            .into_iter()
            .filter(|(portal, way)| way.open && voxear_scene::dimension_of(world, *portal) == here)
            .filter_map(|(portal, way)| Some((portal, way.to.filter(|to| world.is_alive(*to))?)))
            .filter(|(_, to)| voxear_scene::dimension_of(world, *to) == into)
            .any(|(portal, to)| {
                let near = |obj: Entity| voxear_scene::world_matrix(world, obj).w_axis.truncate().distance(place);
                near(portal).min(near(to)) <= CROSSING_REACH
            });
        let ready = into.is_level() || viewer.dimensions.ready.contains(&into);
        if by_portal && ready {
            voxear_scene::put_in_dimension(world, entity, into);
            continue;
        }
        tracing::warn!(
            "player {}'s machine took an Obj into a dimension with no Portal by it there; put back",
            player.0
        );
        let truth = viewer.dimensions.number(here).unwrap_or(0);
        let client = session.by_client.iter().find(|(_, p)| **p == player).map(|(c, _)| *c);
        if let (Some(client), End::Server(server)) = (client, &mut session.end) {
            let message = Message::Crossed { id, dimension: truth, tick: session.tick };
            let _ = server.send(client, Channel::RELIABLE, &message.encode(session.origin));
        }
    }
}

/// The dimensions the server named that this client has not made yet, to
/// find or make: each by its number and what it is.
pub fn to_make(world: &World) -> Vec<(u16, DimensionWhat)> {
    let Some(session) = world.resource::<NetSession>().filter(|s| matches!(s.end, End::Client(_))) else {
        return Vec::new();
    };
    let made = |id: DimensionId| world.resource::<crate::dimensions::Dimensions>().is_some_and(|d| d.exists(id));
    session
        .dims
        .held
        .iter()
        .filter(|(_, held)| held.kept && !held.failed && !held.local.is_some_and(made))
        .map(|(number, held)| (*number, held.what.clone()))
        .collect()
}

/// Notes the dimension this client found or made for the server's number.
pub fn made(world: &mut World, number: u16, local: DimensionId) {
    if let Some(held) = world.resource_mut::<NetSession>().and_then(|s| s.dims.held.get_mut(&number)) {
        held.local = Some(local);
    }
}

/// Notes a dimension the server named that could not be made here, so it
/// is not tried again until named again.
pub fn failed(world: &mut World, number: u16) {
    if let Some(held) = world.resource_mut::<NetSession>().and_then(|s| s.dims.held.get_mut(&number)) {
        held.failed = true;
    }
}

/// The edits that came for dimensions this client has made, in the order
/// they came, for the game to make; and as all of a dimension's are taken,
/// tells the server it is ready. Those for a dimension not made yet wait.
pub fn take_edits(world: &mut World) -> Vec<ArrivedEdits> {
    let Some(session) = world.resource_mut::<NetSession>().filter(|s| matches!(s.end, End::Client(_))) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    while let Some(front) = session.dims.incoming.front() {
        let number = match front {
            Incoming::Edits(number, _) | Incoming::Done(number) => *number,
        };
        let Some(local) = session.dims.local(number) else { break };
        match session.dims.incoming.pop_front() {
            Some(Incoming::Edits(_, piece)) => out.push(ArrivedEdits { dimension: local, piece }),
            Some(Incoming::Done(_)) => {
                if let Some(held) = session.dims.held.get_mut(&number) {
                    held.edits_in = true;
                }
                if let End::Client(client) = &mut session.end {
                    let _ = client.send(Channel::RELIABLE, &Message::Ready(number).encode(session.origin));
                }
            }
            None => break,
        }
    }
    out
}

/// Whether a dimension's edits are in on this machine, so its Portals may
/// open: on a client, the level's, or one the server named whose edits all
/// came; anywhere else, every one.
pub fn edits_in(world: &World, dimension: DimensionId) -> bool {
    let Some(session) = world.resource::<NetSession>().filter(|s| matches!(s.end, End::Client(_))) else {
        return true;
    };
    dimension.is_level()
        || session.dims.held.values().find(|held| held.local == Some(dimension)).is_some_and(|held| held.edits_in)
}

/// Whether an Obj may go through a Portal into `into` now: on a server, a
/// remote player's own only once their machine has the dimension's edits.
pub fn may_cross(world: &World, entity: Entity, into: DimensionId) -> bool {
    let Some(session) = world.resource::<NetSession>() else { return true };
    match session.end {
        End::Server(_) => {
            let owner = world.get::<Networked>(entity).and_then(|networked| networked.owner);
            let Some(owner) = owner.filter(|owner| Some(*owner) != session.local) else { return true };
            let viewer = session.replication.viewers.get(&owner);
            into.is_level() || viewer.is_none_or(|viewer| viewer.dimensions.ready.contains(&into))
        }
        End::Client(_) => edits_in(world, into),
    }
}

/// Whether this machine moves an Obj through Portals itself: one it has
/// authority over or predicts. The rest go where the server says.
pub fn moves_here(world: &World, entity: Entity) -> bool {
    world.resource::<NetSession>().is_none()
        || !world.has::<Networked>(entity)
        || super::has_authority(world, entity)
        || world.has::<super::Predicting>(entity)
}

/// Each tick on a client: the Objs waiting for their dimensions made now,
/// and crossings heard for Objs this machine runs or predicts put on them at
/// once; the rest are put on as their tick is shown.
pub(super) fn settle_client(world: &mut World, session: &mut NetSession) {
    let dims = &mut session.dims;
    let replication = &mut session.replication;
    let waiting = std::mem::take(&mut dims.waiting);
    for (number, mut request) in waiting {
        match dims.local(number) {
            Some(local) => {
                request.dimension = local;
                replication.spawns.push(request);
            }
            None => dims.waiting.push((number, request)),
        }
    }
    for (id, number) in std::mem::take(&mut dims.late_crossings) {
        match replication.remote.get_mut(&id) {
            Some(remote) => {
                remote.dimension = number;
                remote.crossings.push_back((0, number));
            }
            None if replication.spawns.iter().any(|s| s.id == id) || dims.waiting.iter().any(|(_, s)| s.id == id) => {
                dims.late_crossings.push((id, number));
            }
            None => {}
        }
    }
    let own: HashSet<NetId> = session.prediction.ids.union(&session.authority.owned).copied().collect();
    // Shown once the server's tick it went on is; or, where no snapshot shows the server's ticks going by, as
    // nothing moves, once this machine is well past it.
    let past = session.tick.saturating_sub(SHOWN_WITHOUT_SNAPSHOTS);
    for (id, remote) in &mut replication.remote {
        let Some(entity) = remote.entity.filter(|entity| world.is_alive(*entity)) else { continue };
        while let Some(&(tick, number)) = remote.crossings.front() {
            let due = own.contains(id) || tick as f64 <= replication.shown || tick <= past;
            let Some(local) = dims.local(number).filter(|_| due) else { break };
            remote.crossings.pop_front();
            if voxear_scene::dimension_of(world, entity) != local {
                voxear_scene::put_in_dimension(world, entity, local);
            }
        }
    }
}

/// Parks an Obj the server shared in a dimension not made here yet, or
/// hands it to the game to make.
pub(super) fn spawn(session: &mut NetSession, number: u16, mut request: SpawnRequest) {
    match session.dims.local(number) {
        Some(local) => {
            request.dimension = local;
            session.replication.spawns.push(request);
        }
        None => session.dims.waiting.push((number, request)),
    }
}

/// Tells the server, from a client whose player runs their own Objs, each
/// of them that went into another dimension here.
pub(super) fn send_own_crossings(world: &World, session: &mut NetSession) {
    if session.settings.authority != NetAuthority::Shared || !session.welcomed {
        return;
    }
    let mut said = Vec::new();
    for id in &session.authority.owned {
        let Some(remote) = session.replication.remote.get(id) else { continue };
        let Some(entity) = remote.entity.filter(|entity| world.is_alive(*entity)) else { continue };
        let here = voxear_scene::dimension_of(world, entity);
        let Some(number) = session.dims.number_of(here) else { continue };
        if number != remote.dimension && remote.crossings.is_empty() {
            said.push((*id, number));
        }
    }
    let End::Client(client) = &mut session.end else { return };
    for (id, number) in said {
        let message = Message::Crossed { id, dimension: number, tick: session.tick };
        let _ = client.send(Channel::RELIABLE, &message.encode(session.origin));
        if let Some(remote) = session.replication.remote.get_mut(&id) {
            remote.dimension = number;
        }
    }
}

/// Puts a predicted Obj back in the dimension the server has it in, as it
/// is put back where the server has it.
pub(super) fn put_back(world: &mut World, session: &NetSession, id: NetId, entity: Entity) {
    let Some(remote) = session.replication.remote.get(&id) else { return };
    if let Some(local) = session.dims.local(remote.dimension)
        && voxear_scene::dimension_of(world, entity) != local
    {
        voxear_scene::put_in_dimension(world, entity, local);
    }
}
