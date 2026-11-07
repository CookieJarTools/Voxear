//! RPCs: a method of a networked Obj's behaviour, run elsewhere. A
//! player's machine asks the server to run one, and the server runs one on
//! players' machines. Each goes reliably, in order, unless the script says
//! it may be lost, and carries the tick it was made on.
//!
//! The server takes only so many from each player a second, and drops the
//! rest. Whether a player may call a method at all, as its Obj's owner, is
//! for the script on the server to say, since only it knows the method.
//! A call the server makes reaches the players who have the Obj, so one made
//! just after the Obj, in the same tick, still reaches those it is made for.

use std::collections::{HashMap, VecDeque};

use voxear_ecs::Entity;

use super::{NetId, PlayerId};

/// Where an RPC runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RpcTarget {
    Server,
    /// Every player's machine that has the Obj, a host's own too.
    Players,
    Player(PlayerId),
}

/// An RPC that came, for scripts to run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rpc {
    pub entity: Entity,
    /// The method, by a key its script gives it.
    pub key: u32,
    /// The player who made it; `None` for the server.
    pub caller: Option<PlayerId>,
    /// The tick it was made on, by the clock of the machine that made it.
    pub tick: u64,
    pub bytes: Vec<u8>,
}

/// RPCs a player may make a second, and as many again at once.
const PER_SECOND: f64 = 120.0;
/// Seconds an RPC for an Obj not made yet on this machine waits for it.
const EARLY_WAIT: f64 = 2.0;
/// The most bytes an RPC that may be lost takes: it goes in one packet.
pub const MOST_UNRELIABLE_BYTES: usize = 1000;

/// An RPC the server sends this tick.
pub(super) struct Outgoing {
    pub id: NetId,
    pub key: u32,
    pub target: RpcTarget,
    pub reliable: bool,
    pub bytes: Vec<u8>,
}

#[derive(Default)]
pub(super) struct Rpcs {
    /// Those that came, for scripts to run.
    pub incoming: VecDeque<Rpc>,
    /// The server's, sent once this tick's Objs are made on players' machines.
    pub outgoing: Vec<Outgoing>,
    /// On a client, those for Objs not made yet: when each came, and it.
    pub early: Vec<(f64, NetId, u32, u64, Vec<u8>)>,
    /// Each player's allowance of RPCs, and when it was last topped up.
    allowance: HashMap<PlayerId, (f64, f64)>,
    /// When each player was last told off for making too many.
    warned: HashMap<PlayerId, f64>,
    /// The tick the RPC scripts took last was made on.
    pub tick: u64,
}

impl Rpcs {
    /// Whether `player` may make another RPC now, and if so, counts it.
    pub fn allow(&mut self, player: PlayerId, now: f64) -> bool {
        let (left, at) = self.allowance.entry(player).or_insert((PER_SECOND * 2.0, now));
        *left = (*left + (now - *at) * PER_SECOND).min(PER_SECOND * 2.0);
        *at = now;
        if *left >= 1.0 {
            *left -= 1.0;
            return true;
        }
        let warned = self.warned.entry(player).or_insert(f64::MIN);
        if now - *warned > 1.0 {
            *warned = now;
            tracing::warn!("player {} makes more than {PER_SECOND} RPCs a second; the rest are dropped", player.0);
        }
        false
    }

    pub fn forget(&mut self, player: PlayerId) {
        self.allowance.remove(&player);
        self.warned.remove(&player);
    }

    /// Drops RPCs that waited too long for their Obj.
    pub fn expire(&mut self, now: f64) {
        self.early.retain(|(came, ..)| now - came < EARLY_WAIT);
    }

    /// Those that waited for the Obj with `id`, now made as `entity`.
    pub fn arrived(&mut self, id: NetId, entity: Entity) {
        let (now, waiting): (Vec<_>, Vec<_>) = std::mem::take(&mut self.early).into_iter().partition(|e| e.1 == id);
        self.early = waiting;
        for (_, _, key, tick, bytes) in now {
            self.incoming.push_back(Rpc { entity, key, caller: None, tick, bytes });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_player_making_too_many_has_the_rest_dropped() {
        let mut rpcs = Rpcs::default();
        let player = PlayerId(1);
        let at_once = (0..1000).filter(|_| rpcs.allow(player, 0.0)).count();
        assert_eq!(at_once, 240, "a burst of twice a second's");
        assert!(!rpcs.allow(player, 0.0));
        let later = (0..1000).filter(|_| rpcs.allow(player, 0.5)).count();
        assert_eq!(later, 60, "topped up at a second's rate");
    }
}
