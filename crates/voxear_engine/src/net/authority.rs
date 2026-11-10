//! Shared authority, for games played together: with `authority =
//! "shared"`, each player's machine runs their own Objs, physics and all,
//! and tells the server how they are each tick, with their replicated
//! properties as they change. The server takes their word for it and passes
//! it on to everyone else; the player is sent nothing back about their own.
//! Objs no one owns stay the server's. A player can claim one no one owns,
//! as to pick it up, and the server can hand any Obj to anyone; whoever
//! owns an Obj, every machine hears of it. When a player leaves, the
//! server takes back what was theirs.

use std::collections::{BTreeMap, HashMap, HashSet};

use voxear_ecs::{Component, Entity, World};
use voxear_net::Channel;
use voxear_physics::scene::Rigidbody;

use super::messages::Message;
use super::replication::{Fields, NetId, ObjState, Tracked};
use super::{End, NetAuthority, NetSession, Networked, PlayerId};

/// Whether an Obj's Rigidbody was kinematic before networked play held it
/// still: only the machine with authority over an Obj simulates it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HeldBody {
    pub kinematic: bool,
}

impl Component for HeldBody {}

/// A state's parts a player sends for their own Objs: all but fields,
/// which go reliably as they change.
const OWN_PARTS: u8 = 0b1111;

#[derive(Default)]
pub(super) struct Authority {
    /// On a client, the Objs its player owns and runs.
    pub owned: HashSet<NetId>,
    /// On a server, the newest state each player's machine sent of their
    /// own Objs, by tick.
    states: HashMap<NetId, (u64, ObjState)>,
    /// On a server, players asking for Objs no one owns.
    pub claims: Vec<(NetId, PlayerId)>,
    /// On a server, players who left, whose Objs it takes back.
    pub left: Vec<PlayerId>,
    /// On a server, owners changed since the last tick, for players to
    /// hear of; on a client, those the server told of.
    pub changed_owners: Vec<(NetId, Option<PlayerId>)>,
    pub owners: Vec<(NetId, Option<PlayerId>)>,
}

impl Authority {
    /// Takes a player's word on their own Objs' states, on a tick.
    pub fn states_from(
        &mut self,
        player: PlayerId,
        tick: u64,
        entries: Vec<(NetId, u8, ObjState, Option<Fields>)>,
        objects: &BTreeMap<NetId, Tracked>,
    ) {
        for (id, mask, state, _) in entries {
            let theirs = objects.get(&id).is_some_and(|tracked| tracked.owner == Some(player));
            let newer = self.states.get(&id).is_none_or(|(kept, _)| *kept < tick);
            if theirs && newer && mask & OWN_PARTS == OWN_PARTS {
                self.states.insert(id, (tick, state));
            }
        }
    }
}

/// Holds a networked Obj's Rigidbody still, as a player's machine does
/// with every Obj the server moves.
pub(super) fn hold_body(world: &mut World, entity: Entity) {
    let Some(body) = world.get_mut::<Rigidbody>(entity) else { return };
    let kinematic = body.kinematic;
    body.kinematic = true;
    world.insert(entity, HeldBody { kinematic });
}

/// Lets a held Rigidbody move as it would, or holds it, by whether this
/// machine has authority over its Obj.
fn hold(world: &mut World, entity: Entity, here: bool) {
    let held = world.get::<HeldBody>(entity).copied();
    let Some(body) = world.get_mut::<Rigidbody>(entity) else { return };
    match held {
        Some(held) => body.kinematic = if here { held.kinematic } else { true },
        None if !here => hold_body(world, entity),
        None => {}
    }
}

/// Takes in, at the start of a frame, what changed hands and what players
/// said of their own Objs: a server puts them where their owners say.
pub(super) fn take_in(world: &mut World, session: &mut NetSession) {
    if session.settings.authority != NetAuthority::Shared && session.authority.owners.is_empty() {
        return;
    }
    match session.end {
        End::Server(_) => take_in_server(world, session),
        End::Client(_) => take_in_client(world, session),
    }
}

fn take_in_server(world: &mut World, session: &mut NetSession) {
    let authority = &mut session.authority;
    let objects = &mut session.replication.objects;
    for (id, player) in std::mem::take(&mut authority.claims) {
        if objects.get(&id).is_some_and(|tracked| tracked.owner.is_none()) {
            give(world, objects, &mut authority.changed_owners, id, Some(player));
        }
    }
    for player in std::mem::take(&mut authority.left) {
        let theirs: Vec<NetId> = objects.iter().filter(|(_, t)| t.owner == Some(player)).map(|(id, _)| *id).collect();
        for id in theirs {
            give(world, objects, &mut authority.changed_owners, id, None);
            authority.states.remove(&id);
        }
    }
    let local = session.local;
    for (id, tracked) in objects.iter() {
        let remote = tracked.owner.is_some_and(|owner| Some(owner) != local);
        hold(world, tracked.entity, !remote);
        if let Some((_, state)) = authority.states.get(id).filter(|_| remote) {
            state.apply(world, tracked.entity);
        }
    }
    authority.states.retain(|id, _| objects.contains_key(id));
}

/// Hands an Obj to a player, or to no one, for every machine to hear of.
fn give(
    world: &mut World,
    objects: &mut BTreeMap<NetId, Tracked>,
    changed: &mut Vec<(NetId, Option<PlayerId>)>,
    id: NetId,
    owner: Option<PlayerId>,
) {
    let Some(tracked) = objects.get_mut(&id) else { return };
    tracked.owner = owner;
    if let Some(networked) = world.get_mut::<Networked>(tracked.entity) {
        networked.owner = owner;
    }
    changed.push((id, owner));
}

fn take_in_client(world: &mut World, session: &mut NetSession) {
    for (id, owner) in std::mem::take(&mut session.authority.owners) {
        let entity = session.replication.remote.get(&id).and_then(|remote| remote.entity);
        if let Some(networked) = entity.and_then(|entity| world.get_mut::<Networked>(entity)) {
            networked.owner = owner;
        }
    }
    if session.settings.authority != NetAuthority::Shared {
        return;
    }
    let local = session.local;
    let networked: Vec<(Entity, NetId, bool)> = world
        .query_ref::<(Entity, &Networked)>()
        .into_iter()
        .map(|(entity, networked)| (entity, networked.id, local.is_some() && networked.owner == local))
        .collect();
    session.authority.owned.clear();
    for (entity, id, mine) in networked {
        hold(world, entity, mine);
        if mine {
            session.authority.owned.insert(id);
        }
    }
}

/// Sends the server how this player's own Objs are, at the end of a frame,
/// when players run their own.
pub(super) fn send_owned(world: &World, session: &mut NetSession) {
    if session.settings.authority != NetAuthority::Shared || session.authority.owned.is_empty() || !session.welcomed {
        return;
    }
    let End::Client(client) = &mut session.end else { return };
    let entries: Vec<(NetId, u8, ObjState, Option<Fields>)> = session
        .authority
        .owned
        .iter()
        .filter_map(|id| {
            let entity = session.replication.remote.get(id)?.entity?;
            Some((*id, OWN_PARTS, ObjState::of(world, entity)?, None))
        })
        .collect();
    let message = Message::Snapshot { tick: session.tick, entries };
    let _ = client.send(Channel::UNRELIABLE, &message.encode(session.origin));
}

/// Sends a player's own Obj's replicated properties to the server, from
/// their machine, when players run their own: whether it went.
pub(super) fn send_fields(session: &mut NetSession, id: NetId, key: u32, bytes: Vec<u8>) -> bool {
    if session.settings.authority != NetAuthority::Shared || !session.authority.owned.contains(&id) {
        return false;
    }
    let End::Client(client) = &mut session.end else { return false };
    let fields: Fields = [(key, bytes)].into_iter().collect();
    client.send(Channel::RELIABLE, &Message::Fields { id, fields }.encode(session.origin)).is_ok()
}

/// Whether this machine decides what an Obj does: the server, but for a
/// player's own Objs when players run their own, which their machine
/// decides.
pub fn has_authority(world: &World, entity: Entity) -> bool {
    let Some(session) = world.resource::<NetSession>() else { return true };
    let owner = world.get::<Networked>(entity).and_then(|networked| networked.owner);
    let shared = session.settings.authority == NetAuthority::Shared;
    match session.end {
        End::Server(_) => !(shared && owner.is_some_and(|owner| Some(owner) != session.local)),
        End::Client(_) => shared && owner.is_some() && owner == session.local,
    }
}

/// Asks for an Obj no one owns, for this machine's player, when players
/// run their own: a client asks the server, and a host takes it.
pub fn claim(world: &mut World, entity: Entity) {
    let Some(networked) = world.get::<Networked>(entity).copied() else { return };
    let Some(session) = world.resource_mut::<NetSession>() else { return };
    if session.settings.authority != NetAuthority::Shared || networked.owner.is_some() {
        return;
    }
    let local = session.local;
    match &mut session.end {
        End::Client(client) => {
            drop(client.send(Channel::RELIABLE, &Message::Claim(networked.id).encode(session.origin)));
        }
        End::Server(_) => {
            if let Some(local) = local {
                session.authority.claims.push((networked.id, local));
            }
        }
    }
}
