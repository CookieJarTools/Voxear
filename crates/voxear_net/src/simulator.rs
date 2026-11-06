//! A bad network on demand: wraps any transport and makes what it sends
//! late, uneven, lost, doubled, out of order and slow, as far as its
//! [`Conditions`] say. Wrapped at both ends, both ways suffer.

use std::net::SocketAddr;

use crate::transport::Transport;

/// How bad the network is, one way.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Conditions {
    /// Seconds each packet is held back.
    pub latency: f64,
    /// Up to how many seconds more, at random, so packets pass each other.
    pub jitter: f64,
    /// The share of packets lost.
    pub loss: f64,
    /// The share of packets that arrive twice.
    pub duplicate: f64,
    /// How many bytes a second get through at most; 0 for no limit.
    pub bytes_per_second: f64,
}

impl Conditions {
    pub const PERFECT: Self = Self { latency: 0.0, jitter: 0.0, loss: 0.0, duplicate: 0.0, bytes_per_second: 0.0 };
    /// A nearby server on a wired connection.
    pub const GOOD: Self = Self { latency: 0.015, jitter: 0.003, loss: 0.002, duplicate: 0.0, bytes_per_second: 0.0 };
    /// Across a country, on home broadband.
    pub const AVERAGE: Self = Self { latency: 0.04, jitter: 0.01, loss: 0.01, duplicate: 0.002, bytes_per_second: 0.0 };
    /// Far away on a busy wireless connection.
    pub const BAD: Self =
        Self { latency: 0.1, jitter: 0.04, loss: 0.05, duplicate: 0.01, bytes_per_second: 256.0 * 1024.0 };
}

impl Default for Conditions {
    fn default() -> Self {
        Self::PERFECT
    }
}

/// A transport whose sends pass through [`Conditions`].
pub struct Simulated<T: Transport> {
    inner: T,
    conditions: Conditions,
    held: Vec<(f64, SocketAddr, Vec<u8>)>,
    now: f64,
    dice: u64,
    /// When the link is next free, for the byte rate.
    free_at: f64,
}

impl<T: Transport> Simulated<T> {
    /// `seed` picks which packets suffer; the same seed, the same packets.
    pub fn new(inner: T, conditions: Conditions, seed: u64) -> Self {
        Self { inner, conditions, held: Vec::new(), now: 0.0, dice: seed | 1, free_at: 0.0 }
    }

    pub fn set_conditions(&mut self, conditions: Conditions) {
        self.conditions = conditions;
    }

    pub fn inner(&self) -> &T {
        &self.inner
    }

    fn roll(&mut self) -> f64 {
        self.dice ^= self.dice << 13;
        self.dice ^= self.dice >> 7;
        self.dice ^= self.dice << 17;
        (self.dice >> 11) as f64 / (1u64 << 53) as f64
    }
}

impl<T: Transport> Transport for Simulated<T> {
    fn send(&mut self, to: SocketAddr, packet: &[u8]) {
        let c = self.conditions;
        if self.roll() < c.loss {
            return;
        }
        let mut at = self.now + c.latency + self.roll() * c.jitter;
        if c.bytes_per_second > 0.0 {
            // Queued behind what is already on the link; too far behind and it is dropped, as a full router would.
            let start = self.free_at.max(self.now);
            if start - self.now > 0.25 {
                return;
            }
            self.free_at = start + packet.len() as f64 / c.bytes_per_second;
            at = at.max(self.free_at + c.latency);
        }
        if self.roll() < c.duplicate {
            let again = at + self.roll() * 0.02;
            self.held.push((again, to, packet.to_vec()));
        }
        self.held.push((at, to, packet.to_vec()));
    }

    fn receive(&mut self, buffer: &mut [u8]) -> Option<(usize, SocketAddr)> {
        self.inner.receive(buffer)
    }

    fn flush(&mut self) {
        let now = self.now;
        let mut i = 0;
        while i < self.held.len() {
            if self.held[i].0 <= now {
                let (_, to, packet) = self.held.swap_remove(i);
                self.inner.send(to, &packet);
            } else {
                i += 1;
            }
        }
        self.inner.flush();
    }

    fn local_addr(&self) -> SocketAddr {
        self.inner.local_addr()
    }

    fn update(&mut self, now: f64) {
        self.now = now;
        self.inner.update(now);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::MemoryNetwork;

    #[test]
    fn packets_arrive_late_and_some_never() {
        let network = MemoryNetwork::new();
        let conditions = Conditions { latency: 0.1, jitter: 0.0, loss: 0.25, duplicate: 0.0, bytes_per_second: 0.0 };
        let mut a = Simulated::new(network.bind("10.0.0.1:1".parse().unwrap()), conditions, 3);
        let mut b = network.bind("10.0.0.2:1".parse().unwrap());
        a.update(0.0);
        for i in 0..400u16 {
            a.send(b.local_addr(), &i.to_le_bytes());
        }
        a.flush();
        assert!(b.receive(&mut [0; 4]).is_none(), "not yet");
        a.update(0.1);
        a.flush();
        let mut count = 0;
        while b.receive(&mut [0; 4]).is_some() {
            count += 1;
        }
        assert!((260..340).contains(&count), "about three in four: {count}");
    }
}
