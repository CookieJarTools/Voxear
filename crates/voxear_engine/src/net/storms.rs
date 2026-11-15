//! Storms in a session: the server's clock, the storms its scripts made and
//! its whirlpools, sent to each player as they join, again whenever they
//! change, and every few seconds besides, so every machine's storms stand
//! and blow the same. Natural storms are worked out from the world's seed
//! and the clock alone, so the clock is all they need.
//!
//! A client's own made storms last only until the server's next arrive: in
//! a session the server's are the storms.

use voxear_ecs::World;
use voxear_net::ClientId;

use super::{Message, NetRole, NetSession};
use crate::storms::{Storms, StormsSaved};

/// How often the server sends its storms when nothing about them changed,
/// in seconds, so every machine's clock keeps with its own.
pub const RESEND: f64 = 5.0;
/// Where the clock is in the storms' bytes, which is left out when telling
/// whether they changed.
const CLOCK: std::ops::Range<usize> = 4..12;

/// What a session knows of the storms: on a server, what it last sent and
/// when, and who has joined since and is owed them; on a client, what last
/// arrived and how long it took to come.
#[derive(Default)]
pub(super) struct StormSync {
    sent: Option<(Vec<u8>, f64)>,
    owed: Vec<ClientId>,
    arrived: Option<(Vec<u8>, f64)>,
}

impl StormSync {
    /// A player joined: they are sent the storms with what is sent next.
    pub(super) fn owe(&mut self, client: ClientId) {
        self.owed.push(client);
    }

    /// The storms arrived from the server, half the round trip ago.
    pub(super) fn arrived(&mut self, bytes: Vec<u8>, rtt: f64) {
        self.arrived = Some((bytes, rtt * 0.5));
    }
}

/// Whether two sets of the storms' bytes differ, their clocks aside.
fn changed(a: &[u8], b: &[u8]) -> bool {
    let without_clock =
        |bytes: &[u8]| (bytes.get(..CLOCK.start).map(<[u8]>::to_vec), bytes.get(CLOCK.end..).map(<[u8]>::to_vec));
    without_clock(a) != without_clock(b)
}

/// On a server: sends the storms to whoever joined, and to everyone when
/// they changed or a while has passed.
pub(super) fn send(world: &World, session: &mut NetSession) {
    if session.role == NetRole::Client {
        return;
    }
    let Some(storms) = world.resource::<Storms>() else { return };
    let bytes = storms.saved().to_bytes();
    if bytes.len() > super::messages::MOST_STORM_BYTES {
        return;
    }
    let now = session.now;
    let due = match &session.storms.sent {
        None => true,
        Some((sent, at)) => changed(sent, &bytes) || now - at >= RESEND,
    };
    let message = Message::Storms(bytes.clone());
    if due {
        session.send_all(&message, None);
        session.storms.owed.clear();
        session.storms.sent = Some((bytes, now));
        return;
    }
    for client in std::mem::take(&mut session.storms.owed) {
        session.send(client, voxear_net::Channel::RELIABLE, &message);
    }
}

/// On a client: puts the storms as the server last sent them in place, their
/// clock moved on by as long as they took to come.
pub(super) fn take_in(world: &mut World, session: &mut NetSession) {
    let Some((bytes, late)) = session.storms.arrived.take() else { return };
    let Some(mut saved) = StormsSaved::from_bytes(&bytes) else {
        tracing::warn!("the server's storms could not be read");
        return;
    };
    saved.clock += late;
    let mut storms = world.remove_resource::<Storms>().unwrap_or_default();
    storms.restore(saved);
    world.insert_resource(storms);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn storms_count_as_changed_by_what_they_are_not_by_their_clock() {
        let mut storms = Storms::default();
        storms.clock = 10.0;
        let before = storms.saved().to_bytes();
        storms.clock = 99.0;
        assert!(!changed(&before, &storms.saved().to_bytes()));
        storms.start(crate::storms::Storm::default());
        assert!(changed(&before, &storms.saved().to_bytes()));
    }
}
