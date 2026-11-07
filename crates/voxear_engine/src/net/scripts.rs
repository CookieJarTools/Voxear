//! What answers `Net` in scripts (`NativeNet` in Voxear.Core numbers the
//! calls).

use std::net::SocketAddr;

use voxear_ecs::World;

use super::{NetEvent, NetId, NetRole, NetSession, NetSettings, PlayerId, RpcTarget};

/// The project's network settings, kept in the world for scripts that
/// start a session.
#[derive(Clone, Debug)]
pub struct NetDefaults(pub NetSettings);

pub(crate) fn register() {
    voxear_scripting::set_net_calls(call);
    voxear_scripting::set_net_object_values(net_object);
    voxear_scripting::set_net_fields(fields);
    voxear_scripting::set_net_rpcs(voxear_scripting::NetRpcs { send: send_rpc, take: take_rpc });
    voxear_scripting::set_net_raycast(|world, player, tick, origin, direction, distance, mask, triggers| {
        super::raycast_as_seen_by(world, PlayerId(player), tick, origin, direction, distance, mask, triggers)
    });
}

fn send_rpc(world: &mut World, rpc: voxear_scripting::RpcSend) -> bool {
    let target = match (rpc.to_players, rpc.player) {
        (false, _) => RpcTarget::Server,
        (true, None) => RpcTarget::Players,
        (true, Some(player)) => RpcTarget::Player(PlayerId(player)),
    };
    super::rpc(world, rpc.entity, rpc.key, rpc.bytes, target, rpc.reliable)
}

fn take_rpc(world: &mut World) -> Option<voxear_scripting::RpcTaken> {
    let rpc = super::take_rpc(world)?;
    Some(voxear_scripting::RpcTaken { entity: rpc.entity, key: rpc.key, caller: rpc.caller.map(|p| p.0), bytes: rpc.bytes })
}

/// The most bytes one behaviour's replicated fields take.
const MOST_FIELD_BYTES: usize = 4096;

/// A behaviour's replicated fields given on a server, or with `None`, the
/// next that came from the server.
fn fields(
    world: &mut World,
    given: Option<(voxear_ecs::Entity, u32, Vec<u8>)>,
) -> Option<(voxear_ecs::Entity, u32, Vec<u8>)> {
    match given {
        Some((entity, key, bytes)) if bytes.len() <= MOST_FIELD_BYTES => {
            super::set_fields(world, entity, key, bytes);
            None
        }
        Some(_) => {
            tracing::error!(target: "script", "a behaviour's replicated fields come to more than {MOST_FIELD_BYTES} bytes");
            None
        }
        None => {
            let updates = &mut world.resource_mut::<NetSession>()?.replication.field_updates;
            (!updates.is_empty()).then(|| updates.remove(0))
        }
    }
}

// What `net_object` reads or writes, as `NativeNetObject` in Voxear.Core numbers them.
const HAS: i32 = 0;
const ADD: i32 = 1;
const REMOVE: i32 = 2;
const PRIORITY: i32 = 3;
const RANGE: i32 = 4;
const ID: i32 = 5;
const OWNER: i32 = 6;
const HAS_AUTHORITY: i32 = 7;
const CLAIM: i32 = 8;

/// Reads value `what` of `entity`'s Net Object, or with `written` writes
/// it: `None` when it has none, or is gone.
fn net_object(world: &mut World, entity: voxear_ecs::Entity, what: i32, written: Option<[f32; 4]>) -> Option<[f32; 4]> {
    use super::{NetObject, Networked};
    if !world.is_alive(entity) {
        return None;
    }
    let number = |n: f32| [n, 0.0, 0.0, 0.0];
    match (what, written) {
        (HAS, None) => return Some(number(f32::from(u8::from(world.has::<NetObject>(entity))))),
        (ADD, Some(_)) => {
            if !world.has::<NetObject>(entity) {
                world.insert(entity, NetObject::default());
            }
            return Some([0.0; 4]);
        }
        (REMOVE, Some(_)) => return world.remove::<NetObject>(entity).map(|_| [0.0; 4]),
        (ID, None) => return Some(number(world.get::<Networked>(entity).map_or(0.0, |n| n.id.0 as f32))),
        (OWNER, None) => {
            let waiting = world.get::<super::WaitingOwner>(entity).and_then(|w| w.0);
            let owner = world.get::<Networked>(entity).and_then(|n| n.owner).or(waiting);
            return Some(number(owner.map_or(-1.0, |p| f32::from(p.0))));
        }
        (HAS_AUTHORITY, None) => return Some(number(f32::from(u8::from(super::has_authority(world, entity))))),
        (CLAIM, Some(_)) => {
            super::claim(world, entity);
            return Some([0.0; 4]);
        }
        (OWNER, Some([x, ..])) => {
            let owner = (x >= 0.0).then(|| PlayerId(x as u16));
            super::set_owner(world, entity, owner);
            return Some([0.0; 4]);
        }
        _ => {}
    }
    let object = world.get_mut::<NetObject>(entity)?;
    match (what, written) {
        (PRIORITY, None) => Some(number(object.priority)),
        (RANGE, None) => Some(number(object.range)),
        (PRIORITY, Some([x, ..])) => {
            object.priority = x.max(0.0);
            Some([0.0; 4])
        }
        (RANGE, Some([x, ..])) => {
            object.range = x.max(0.0);
            Some([0.0; 4])
        }
        _ => None,
    }
}

const ROLE: i32 = 0;
const CONNECTED: i32 = 1;
const LOCAL_PLAYER: i32 = 2;
const PLAYER_COUNT: i32 = 3;
const PLAYER_AT: i32 = 4;
const START_SERVER: i32 = 5;
const START_HOST: i32 = 6;
const CONNECT: i32 = 7;
const STOP: i32 = 8;
const TAKE_EVENT: i32 = 9;
const INPUT: i32 = 10;
const TICK: i32 = 11;
const ROUND_TRIP: i32 = 12;
const RPC_TICK: i32 = 13;
const NET_ID: i32 = 14;
const OBJ_INDEX: i32 = 15;
const OBJ_GENERATION: i32 = 16;
const REPLAYING: i32 = 17;
const AUTHORITY: i32 = 18;
const AIM: i32 = 19;
const SET_AIM: i32 = 20;

// Events, as TAKE_EVENT gives them.
const CONNECTED_EVENT: i32 = 1;
const DISCONNECTED_EVENT: i32 = 2;
const JOINED_EVENT: i32 = 3;
const LEFT_EVENT: i32 = 4;

fn call(world: &mut World, what: i32, a: i32, b: i32, text: &str, out: &mut [f32; 4]) -> i32 {
    let session = world.resource::<NetSession>();
    match what {
        ROLE => match session.map(NetSession::role) {
            None => 0,
            Some(NetRole::Server) => 1,
            Some(NetRole::Host) => 2,
            Some(NetRole::Client) => 3,
        },
        CONNECTED => i32::from(session.is_some_and(NetSession::is_connected)),
        LOCAL_PLAYER => session.and_then(NetSession::local_player).map_or(-1, |p| i32::from(p.0)),
        PLAYER_COUNT => session.map_or(0, |s| s.players().len() as i32),
        PLAYER_AT => session.and_then(|s| s.players().get(a as usize)).map_or(-1, |p| i32::from(p.0)),
        TICK => session.map_or(0, |s| s.tick() as i32),
        RPC_TICK => super::rpc_tick(world) as i32,
        REPLAYING => i32::from(super::replaying(world)),
        AIM => {
            let aim = session.map_or([0.0; 2], |s| s.aim(PlayerId(a as u16)));
            *out = [aim[0], aim[1], 0.0, 0.0];
            1
        }
        // The aim's two numbers come as their bits.
        SET_AIM => {
            super::set_aim(world, [f32::from_bits(a as u32), f32::from_bits(b as u32)]);
            1
        }
        AUTHORITY => i32::from(session.is_some_and(|s| s.settings.authority == super::NetAuthority::Shared)),
        // An Obj, as its handle's two halves, by its id in the session; and back.
        NET_ID => {
            let entity = voxear_ecs::Entity::from_bits(u64::from(b as u32) << 32 | u64::from(a as u32));
            world.get::<super::Networked>(entity).map_or(0, |n| n.id.0 as i32)
        }
        OBJ_INDEX | OBJ_GENERATION => {
            let Some(entity) = super::entity_of(world, NetId(a as u32)) else { return 0 };
            if what == OBJ_INDEX { entity.index() as i32 } else { entity.generation() as i32 }
        }
        ROUND_TRIP => {
            out[0] = session.map_or(0.0, |s| s.round_trip() as f32);
            1
        }
        INPUT => {
            let Some(session) = session else { return 0 };
            let state = session.input(PlayerId(a as u16), b as usize);
            let flags = u8::from(state.held) | u8::from(state.pressed) << 1 | u8::from(state.released) << 2
                | u8::from(state.performed) << 3;
            *out = [state.value[0], state.value[1], f32::from(flags), 0.0];
            1
        }
        TAKE_EVENT => {
            let Some(session) = world.resource_mut::<NetSession>() else { return 0 };
            let Some(event) = session.events.pop_front() else { return 0 };
            let (kind, player) = match event {
                NetEvent::Connected => (CONNECTED_EVENT, None),
                NetEvent::Disconnected => (DISCONNECTED_EVENT, None),
                NetEvent::PlayerJoined(player) => (JOINED_EVENT, Some(player)),
                NetEvent::PlayerLeft(player) => (LEFT_EVENT, Some(player)),
            };
            out[0] = player.map_or(-1.0, |p| f32::from(p.0));
            kind
        }
        START_SERVER | START_HOST | CONNECT => {
            let Some(NetDefaults(settings)) = world.resource::<NetDefaults>().cloned() else {
                tracing::error!(target: "script", "networked play needs the game running");
                return 0;
            };
            super::stop(world);
            let port = if a <= 0 { settings.port } else { a as u16 };
            let started = match what {
                START_SERVER => super::serve(world, settings, port, false).map(drop),
                START_HOST => super::serve(world, settings, port, true).map(drop),
                _ => address(text, settings.port).and_then(|address| super::connect(world, settings, address)),
            };
            match started {
                Ok(()) => 1,
                Err(error) => {
                    tracing::error!(target: "script", "{error:#}");
                    0
                }
            }
        }
        STOP => {
            super::stop(world);
            1
        }
        _ => 0,
    }
}

/// An address, with the project's port when it names none.
fn address(text: &str, port: u16) -> anyhow::Result<SocketAddr> {
    use std::net::ToSocketAddrs;
    let text = text.trim();
    let with_port = if text.contains(':') && !text.starts_with('[') && text.matches(':').count() == 1
        || text.starts_with('[') && text.contains("]:")
    {
        text.to_string()
    } else {
        format!("{text}:{port}")
    };
    with_port
        .to_socket_addrs()
        .map_err(anyhow::Error::from)
        .and_then(|mut found| found.next().ok_or_else(|| anyhow::anyhow!("no such place")))
        .map_err(|error| anyhow::anyhow!("{text} is not an address to join: {error}"))
}
