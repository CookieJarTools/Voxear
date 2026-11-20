//! Crowd records in a session: the server sends each player the records
//! within drawing distance of them (Project Settings ▸ Crowds ▸ Draw
//! records within), as they join, whenever records are made or made Objs
//! again, and every few seconds besides; their machine walks them on
//! between and draws them far off. The records are the server's: a
//! player's machine never makes or takes any.

use std::collections::HashMap;

use voxear_ecs::World;
use voxear_net::ClientId;

use super::{Message, NetRole, NetSession};

/// How often each player is sent the records near them when none were made
/// or taken, in seconds, so they keep where the server's are and follow
/// the player about; and the least time between sends.
pub const RESEND: f64 = 5.0;
pub const LEAST: f64 = 1.0;

/// On a server: each player's last records sent, as the records' version
/// and when; on a client, what last arrived.
#[derive(Default)]
pub(super) struct RecordSync {
    sent: HashMap<ClientId, (u64, f64)>,
    arrived: Option<Vec<u8>>,
}

impl RecordSync {
    /// The records arrived from the server.
    pub(super) fn arrived(&mut self, bytes: Vec<u8>) {
        self.arrived = Some(bytes);
    }

    /// A player left.
    pub(super) fn forget(&mut self, client: ClientId) {
        self.sent.remove(&client);
    }
}

/// On a server: sends each player the records near them when they have
/// none yet, when records changed, or when a while has passed.
pub(super) fn send(world: &World, session: &mut NetSession) {
    if session.role == NetRole::Client || crate::crowd_records::count(world) == 0 && session.records.sent.is_empty() {
        return;
    }
    let within =
        world.resource::<crate::crowds::CrowdSettings>().map_or(600.0, |settings| settings.draw_records_within);
    let now = session.now;
    let clients: Vec<(ClientId, super::PlayerId)> = session.by_client.iter().map(|(c, p)| (*c, *p)).collect();
    for (client, player) in clients {
        let Some((at, _)) = session.replication.viewpoint(player) else { continue };
        let version = world.resource::<crate::crowd_records::CrowdRecords>().map_or(0, |records| records.version);
        let due = match session.records.sent.get(&client) {
            None => true,
            Some(&(sent, when)) => (sent != version && now - when >= LEAST) || now - when >= RESEND,
        };
        if !due || within <= 0.0 {
            continue;
        }
        let (bytes, version) = crate::crowd_records::near_bytes(world, at, within, super::messages::MOST_STORM_BYTES);
        session.send(client, voxear_net::Channel::RELIABLE, &Message::CrowdRecords(bytes));
        session.records.sent.insert(client, (version, now));
    }
}

/// On a client: puts the records the server last sent in place.
pub(super) fn take_in(world: &mut World, session: &mut NetSession) {
    let Some(bytes) = session.records.arrived.take() else { return };
    if !crate::crowd_records::take_from_server(world, &bytes) {
        tracing::warn!("the server's crowd records could not be read");
    }
}
