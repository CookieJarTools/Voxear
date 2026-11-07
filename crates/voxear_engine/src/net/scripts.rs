//! What answers `Net` in scripts (`NativeNet` in Voxear.Core numbers the
//! calls).

use std::net::SocketAddr;

use voxear_ecs::World;

use super::{NetEvent, NetRole, NetSession, NetSettings, PlayerId};

/// The project's network settings, kept in the world for scripts that
/// start a session.
#[derive(Clone, Debug)]
pub struct NetDefaults(pub NetSettings);

pub(crate) fn register() {
    voxear_scripting::set_net_calls(call);
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
