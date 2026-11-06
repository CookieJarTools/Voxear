//! One end of a connection, below the encryption: what makes messages
//! arrive. Every packet carries its own sequence number and acknowledges the
//! last 33 it got, so acknowledgements never need packets of their own and
//! survive any one being lost.
//!
//! Messages go on channels:
//! - **unreliable**, sent once in the next packet with room, the way state
//!   that is soon out of date is sent; a tagged one reports whether its
//!   packet arrived, so the sender can send what it carried again if not;
//! - **reliable, ordered**, sent again until acknowledged and handed over
//!   in order, for spawns, events and anything sent once. A large one is
//!   sliced into pieces that are put back together;
//! - **reliable, unordered**, handed over as each arrives.
//!
//! Packets are at most [`MAX_PAYLOAD`] bytes, so no router splits them. A
//! packet a large unreliable message makes bigger goes as up to
//! [`MAX_FRAGMENTS`] fragments. Each end sends no more than its byte budget
//! a second; what doesn't fit waits for the next flush.

use std::collections::VecDeque;

use crate::sequence::{SequenceBuffer, newer};

/// The most bytes a connection packet holds, leaving room in a 1200-byte
/// datagram for the encryption round it.
pub const MAX_PAYLOAD: usize = 1168;
/// The most fragments a packet is cut into.
pub const MAX_FRAGMENTS: usize = 4;
/// The size of each slice a large reliable message is cut into.
const SLICE: usize = 1024;
/// How many reliable messages may be on their way at once.
const MESSAGE_WINDOW: u16 = 1024;
/// How many sent and received packets are remembered.
const PACKET_WINDOW: usize = 1024;
/// A packet unacknowledged this long is counted lost.
const LOST_AFTER: f64 = 1.0;
/// Bytes of a packet's header: kind, sequence, ack and ack bits.
const HEADER: usize = 1 + 2 + 2 + 4;
const FRAGMENT_HEADER: usize = 1 + 2 + 1 + 1;

const WHOLE: u8 = 0;
const FRAGMENT: u8 = 1;

/// A channel of a connection, by its place in the connection's channels.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Channel(pub u8);

impl Channel {
    /// The default channels' places (see [`default_channels`]).
    pub const UNRELIABLE: Channel = Channel(0);
    pub const RELIABLE: Channel = Channel(1);
    pub const RELIABLE_UNORDERED: Channel = Channel(2);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChannelKind {
    Unreliable,
    ReliableOrdered,
    ReliableUnordered,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChannelConfig {
    pub kind: ChannelKind,
    /// The most bytes it keeps waiting to be sent, or acknowledged, before
    /// the connection fails rather than grow without end.
    pub max_memory: usize,
}

/// Unreliable, reliable ordered and reliable unordered, in that order.
pub fn default_channels() -> Vec<ChannelConfig> {
    vec![
        ChannelConfig { kind: ChannelKind::Unreliable, max_memory: 256 * 1024 },
        ChannelConfig { kind: ChannelKind::ReliableOrdered, max_memory: 4 * 1024 * 1024 },
        ChannelConfig { kind: ChannelKind::ReliableUnordered, max_memory: 1024 * 1024 },
    ]
}

#[derive(Clone, Debug, PartialEq)]
pub struct ConnectionConfig {
    pub channels: Vec<ChannelConfig>,
    /// How many bytes a second it sends at most.
    pub bytes_per_second: f64,
}

impl Default for ConnectionConfig {
    fn default() -> Self {
        Self { channels: default_channels(), bytes_per_second: 64.0 * 1024.0 }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SendError {
    /// There is no such channel.
    NoChannel,
    /// Larger than the channel can send: an unreliable message fills at
    /// most [`MAX_FRAGMENTS`] fragments, and an unordered one a slice.
    TooLarge,
    /// The channel holds as much as it may; the connection has failed.
    ChannelFull,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PacketError {
    Malformed,
    Duplicate,
}

/// What became of a tagged unreliable message's packet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Delivery {
    Acked(u32),
    Lost(u32),
}

/// How the connection is doing.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Stats {
    /// Round trip, smoothed, in seconds.
    pub rtt: f64,
    /// The share of packets lost, smoothed.
    pub loss: f64,
    pub sent_bytes_per_second: f64,
    pub received_bytes_per_second: f64,
    pub packets_sent: u64,
    pub packets_received: u64,
}

#[derive(Debug)]
struct Outgoing {
    id: u16,
    data: Vec<u8>,
    /// A slice of a larger message with more to follow.
    more: bool,
    last_sent: f64,
    acked: bool,
}

#[derive(Debug, Default)]
struct ChannelState {
    unreliable: Vec<(Vec<u8>, Option<u32>)>,
    pending: VecDeque<Outgoing>,
    next_id: u16,
    memory: usize,
    // Receiving.
    expected: u16,
    held: Option<SequenceBuffer<(Vec<u8>, bool)>>,
    seen: Option<SequenceBuffer<()>>,
    partial: Vec<u8>,
    ready: VecDeque<Vec<u8>>,
}

#[derive(Debug, Default)]
struct SentPacket {
    time: f64,
    reliable: Vec<(u8, u16)>,
    tags: Vec<u32>,
    resolved: bool,
}

#[derive(Debug)]
struct Assembly {
    count: u8,
    parts: Vec<Option<Vec<u8>>>,
}

#[derive(Debug)]
pub struct Connection {
    config: ConnectionConfig,
    channels: Vec<ChannelState>,
    sequence: u16,
    sent: SequenceBuffer<SentPacket>,
    /// The oldest sent packet not yet acknowledged or counted lost.
    unresolved: u16,
    received: SequenceBuffer<()>,
    latest: Option<u16>,
    /// Packets received since one went back to acknowledge them.
    owed: bool,
    fragments: SequenceBuffer<Assembly>,
    deliveries: Vec<Delivery>,
    budget: f64,
    refilled: f64,
    stats: Stats,
    rtt_seeded: bool,
    error: Option<SendError>,
}

impl Connection {
    pub fn new(config: ConnectionConfig, now: f64) -> Self {
        let channels = config
            .channels
            .iter()
            .map(|c| ChannelState {
                held: (c.kind == ChannelKind::ReliableOrdered).then(|| SequenceBuffer::new(MESSAGE_WINDOW as usize)),
                seen: (c.kind == ChannelKind::ReliableUnordered).then(|| SequenceBuffer::new(MESSAGE_WINDOW as usize)),
                ..ChannelState::default()
            })
            .collect();
        let budget = burst(&config);
        Self {
            config,
            channels,
            sequence: 0,
            sent: SequenceBuffer::new(PACKET_WINDOW),
            unresolved: 0,
            received: SequenceBuffer::new(PACKET_WINDOW),
            latest: None,
            owed: false,
            fragments: SequenceBuffer::new(64),
            deliveries: Vec::new(),
            budget,
            refilled: now,
            stats: Stats::default(),
            rtt_seeded: false,
            error: None,
        }
    }

    /// Queues `message` on `channel`.
    pub fn send(&mut self, channel: Channel, message: &[u8]) -> Result<(), SendError> {
        self.queue(channel, message, None)
    }

    /// Queues an unreliable `message` whose packet's fate is reported as a
    /// [`Delivery`] with `tag`.
    pub fn send_tagged(&mut self, channel: Channel, message: &[u8], tag: u32) -> Result<(), SendError> {
        self.queue(channel, message, Some(tag))
    }

    fn queue(&mut self, channel: Channel, message: &[u8], tag: Option<u32>) -> Result<(), SendError> {
        let config = *self.config.channels.get(channel.0 as usize).ok_or(SendError::NoChannel)?;
        let state = &mut self.channels[channel.0 as usize];
        if state.memory + message.len() > config.max_memory {
            self.error = Some(SendError::ChannelFull);
            return Err(SendError::ChannelFull);
        }
        match config.kind {
            ChannelKind::Unreliable => {
                if message.len() > MAX_FRAGMENTS * (MAX_PAYLOAD - FRAGMENT_HEADER) - HEADER - 8 {
                    return Err(SendError::TooLarge);
                }
                state.memory += message.len();
                state.unreliable.push((message.to_vec(), tag));
            }
            ChannelKind::ReliableUnordered if message.len() > SLICE => return Err(SendError::TooLarge),
            _ => {
                let mut slices = message.chunks(SLICE).peekable();
                if message.is_empty() {
                    state.pending.push_back(Outgoing {
                        id: state.next_id,
                        data: Vec::new(),
                        more: false,
                        last_sent: f64::NEG_INFINITY,
                        acked: false,
                    });
                    state.next_id = state.next_id.wrapping_add(1);
                }
                while let Some(slice) = slices.next() {
                    state.pending.push_back(Outgoing {
                        id: state.next_id,
                        data: slice.to_vec(),
                        more: slices.peek().is_some(),
                        last_sent: f64::NEG_INFINITY,
                        acked: false,
                    });
                    state.next_id = state.next_id.wrapping_add(1);
                }
                state.memory += message.len();
            }
        }
        Ok(())
    }

    /// The next message `channel` has received, if any.
    pub fn receive(&mut self, channel: Channel) -> Option<Vec<u8>> {
        self.channels.get_mut(channel.0 as usize)?.ready.pop_front()
    }

    /// What became of tagged messages since last asked.
    pub fn take_deliveries(&mut self) -> Vec<Delivery> {
        std::mem::take(&mut self.deliveries)
    }

    pub fn stats(&self) -> Stats {
        self.stats
    }

    /// Why the connection has failed, if it has.
    pub fn error(&self) -> Option<SendError> {
        self.error
    }

    pub fn set_bytes_per_second(&mut self, rate: f64) {
        self.config.bytes_per_second = rate.max(1200.0);
    }

    /// Takes in one packet the other end sent.
    pub fn receive_packet(&mut self, bytes: &[u8], now: f64) -> Result<(), PacketError> {
        let (&kind, rest) = bytes.split_first().ok_or(PacketError::Malformed)?;
        self.stats.received_bytes_per_second += bytes.len() as f64;
        match kind {
            WHOLE => self.receive_whole(rest, now),
            FRAGMENT => {
                let Some(whole) = self.reassemble(rest)? else { return Ok(()) };
                self.receive_whole(&whole, now)
            }
            _ => Err(PacketError::Malformed),
        }
    }

    fn reassemble(&mut self, bytes: &[u8]) -> Result<Option<Vec<u8>>, PacketError> {
        let [lo, hi, index, count, data @ ..] = bytes else { return Err(PacketError::Malformed) };
        let (sequence, index, count) = (u16::from_le_bytes([*lo, *hi]), *index, *count);
        if count == 0 || count as usize > MAX_FRAGMENTS || index >= count || data.len() > MAX_PAYLOAD {
            return Err(PacketError::Malformed);
        }
        if self.received.contains(sequence) {
            return Err(PacketError::Duplicate);
        }
        if self.fragments.get(sequence).is_none_or(|a| a.count != count) {
            self.fragments.insert(sequence, Assembly { count, parts: vec![None; count as usize] });
        }
        let assembly = self.fragments.get_mut(sequence).ok_or(PacketError::Malformed)?;
        assembly.parts[index as usize] = Some(data.to_vec());
        if assembly.parts.iter().any(Option::is_none) {
            return Ok(None);
        }
        let assembly = self.fragments.remove(sequence).ok_or(PacketError::Malformed)?;
        Ok(Some(assembly.parts.into_iter().flatten().flatten().collect()))
    }

    fn receive_whole(&mut self, bytes: &[u8], now: f64) -> Result<(), PacketError> {
        let mut read = Bytes(bytes);
        let sequence = read.u16()?;
        let ack = read.u16()?;
        let ack_bits = read.u32()?;
        if self.received.contains(sequence) {
            return Err(PacketError::Duplicate);
        }
        // Read every message before taking any in, so a malformed packet changes nothing.
        let mut messages = Vec::new();
        while !read.0.is_empty() {
            let flags = read.u8()?;
            let channel = flags & 0x3F;
            let more = flags & 0x40 != 0;
            let kind = self.config.channels.get(channel as usize).ok_or(PacketError::Malformed)?.kind;
            let id = if kind == ChannelKind::Unreliable { 0 } else { read.u16()? };
            let length = read.varint()? as usize;
            let data = read.take(length)?;
            messages.push((channel, id, more, data));
        }
        self.received.insert(sequence, ());
        if self.latest.is_none_or(|latest| newer(sequence, latest)) {
            self.latest = Some(sequence);
        }
        self.owed = true;
        self.stats.packets_received += 1;
        self.take_acks(ack, ack_bits, now);
        for (channel, id, more, data) in messages {
            self.take_message(channel, id, more, data);
        }
        Ok(())
    }

    fn take_message(&mut self, channel: u8, id: u16, more: bool, data: &[u8]) {
        let state = &mut self.channels[channel as usize];
        if let Some(held) = &mut state.held {
            // Ordered: kept until those before it have come.
            let ahead = id.wrapping_sub(state.expected);
            if ahead >= MESSAGE_WINDOW || held.contains(id) {
                return;
            }
            held.insert(id, (data.to_vec(), more));
            while let Some((data, more)) = held.remove(state.expected) {
                state.expected = state.expected.wrapping_add(1);
                state.partial.extend_from_slice(&data);
                if !more {
                    state.ready.push_back(std::mem::take(&mut state.partial));
                }
            }
        } else if let Some(seen) = &mut state.seen {
            let behind = state.expected.wrapping_sub(id);
            if (MESSAGE_WINDOW..0x8000).contains(&behind) || seen.contains(id) {
                return;
            }
            seen.insert(id, ());
            if newer(id, state.expected) || id == state.expected {
                state.expected = id.wrapping_add(1);
            }
            state.ready.push_back(data.to_vec());
        } else {
            state.ready.push_back(data.to_vec());
        }
    }

    fn take_acks(&mut self, ack: u16, bits: u32, now: f64) {
        for i in 0..33u16 {
            if i > 0 && bits & (1 << (i - 1)) == 0 {
                continue;
            }
            let sequence = ack.wrapping_sub(i);
            let Some(packet) = self.sent.get_mut(sequence) else { continue };
            if packet.resolved {
                continue;
            }
            packet.resolved = true;
            let sample = now - packet.time;
            let reliable = std::mem::take(&mut packet.reliable);
            let tags = std::mem::take(&mut packet.tags);
            if self.rtt_seeded {
                self.stats.rtt += (sample - self.stats.rtt) * 0.1;
            } else {
                (self.stats.rtt, self.rtt_seeded) = (sample, true);
            }
            self.stats.loss *= 0.95;
            for (channel, id) in reliable {
                let state = &mut self.channels[channel as usize];
                if let Some(message) = state.pending.iter_mut().find(|m| m.id == id) {
                    message.acked = true;
                }
                while state.pending.front().is_some_and(|m| m.acked) {
                    let done = state.pending.pop_front().map_or(0, |m| m.data.len());
                    state.memory = state.memory.saturating_sub(done);
                }
            }
            self.deliveries.extend(tags.into_iter().map(Delivery::Acked));
        }
        self.count_lost(Some(ack), now);
    }

    /// Counts packets lost that are too far behind the latest acknowledged,
    /// or too long unacknowledged.
    fn count_lost(&mut self, ack: Option<u16>, now: f64) {
        while self.unresolved != self.sequence {
            let sequence = self.unresolved;
            let behind = ack.is_some_and(|ack| newer(ack, sequence) && ack.wrapping_sub(sequence) > 32);
            match self.sent.get_mut(sequence) {
                Some(packet) if !packet.resolved => {
                    if !behind && now - packet.time < LOST_AFTER {
                        break;
                    }
                    packet.resolved = true;
                    let tags = std::mem::take(&mut packet.tags);
                    self.stats.loss = self.stats.loss * 0.95 + 0.05;
                    self.deliveries.extend(tags.into_iter().map(Delivery::Lost));
                }
                _ => {}
            }
            self.unresolved = self.unresolved.wrapping_add(1);
        }
    }

    /// Sends what is waiting, as far as the budget allows, handing each
    /// packet to `out`. Unreliable messages that do not go now are dropped:
    /// by the next flush they would be out of date.
    pub fn flush(&mut self, now: f64, out: &mut dyn FnMut(&[u8])) {
        let elapsed = (now - self.refilled).max(0.0);
        self.refilled = now;
        self.budget = (self.budget + elapsed * self.config.bytes_per_second).min(burst(&self.config));
        // The sent and received rates, decaying over about a second.
        let decay = (-elapsed).exp();
        self.stats.sent_bytes_per_second *= decay;
        self.stats.received_bytes_per_second *= decay;
        self.count_lost(None, now);
        let resend = (self.stats.rtt * 1.5).clamp(0.05, 0.5);
        let mut unreliable: VecDeque<(u8, Vec<u8>, Option<u32>)> = VecDeque::new();
        for (index, state) in self.channels.iter_mut().enumerate() {
            for (data, tag) in state.unreliable.drain(..) {
                state.memory = state.memory.saturating_sub(data.len());
                unreliable.push_back((index as u8, data, tag));
            }
        }
        loop {
            if self.budget <= 0.0 {
                break;
            }
            let mut body = Vec::with_capacity(MAX_PAYLOAD);
            let mut record = SentPacket { time: now, ..SentPacket::default() };
            self.pack_reliable(&mut body, &mut record, now, resend);
            // Unreliable messages fill what room is left; one too large for any packet goes alone, in fragments.
            let room = |body: &Vec<u8>| MAX_PAYLOAD - HEADER - body.len();
            while let Some((channel, data, tag)) = unreliable.front() {
                let size = 1 + varint_len(data.len() as u64) + data.len();
                let alone = body.is_empty() && record.reliable.is_empty();
                if size > room(&body) && !alone {
                    break;
                }
                body.push(*channel);
                write_varint(&mut body, data.len() as u64);
                body.extend_from_slice(data);
                record.tags.extend(*tag);
                unreliable.pop_front();
                if size > MAX_PAYLOAD - HEADER {
                    break;
                }
            }
            if body.is_empty() && !self.owed {
                break;
            }
            self.send_packet(body, record, out);
            if unreliable.is_empty() && !self.any_due(now, resend) {
                break;
            }
        }
    }

    fn any_due(&self, now: f64, resend: f64) -> bool {
        self.channels.iter().any(|state| {
            state
                .pending
                .iter()
                .take_while(|m| m.id.wrapping_sub(state.pending.front().map_or(0, |f| f.id)) < MESSAGE_WINDOW)
                .any(|m| !m.acked && now - m.last_sent >= resend)
        })
    }

    fn pack_reliable(&mut self, body: &mut Vec<u8>, record: &mut SentPacket, now: f64, resend: f64) {
        for (index, state) in self.channels.iter_mut().enumerate() {
            let Some(first) = state.pending.front().map(|m| m.id) else { continue };
            for message in state.pending.iter_mut() {
                // Never further ahead than the other end keeps.
                if message.id.wrapping_sub(first) >= MESSAGE_WINDOW {
                    break;
                }
                if message.acked || now - message.last_sent < resend {
                    continue;
                }
                let size = 1 + 2 + varint_len(message.data.len() as u64) + message.data.len();
                if body.len() + size > MAX_PAYLOAD - HEADER {
                    return;
                }
                body.push(index as u8 | if message.more { 0x40 } else { 0 });
                body.extend_from_slice(&message.id.to_le_bytes());
                write_varint(body, message.data.len() as u64);
                body.extend_from_slice(&message.data);
                message.last_sent = now;
                record.reliable.push((index as u8, message.id));
            }
        }
    }

    fn send_packet(&mut self, body: Vec<u8>, record: SentPacket, out: &mut dyn FnMut(&[u8])) {
        let sequence = self.sequence;
        self.sequence = self.sequence.wrapping_add(1);
        let (ack, bits) = self.acks();
        self.owed = false;
        let mut whole = Vec::with_capacity(HEADER + body.len());
        whole.extend_from_slice(&sequence.to_le_bytes());
        whole.extend_from_slice(&ack.to_le_bytes());
        whole.extend_from_slice(&bits.to_le_bytes());
        whole.extend_from_slice(&body);
        // A slot reused before its packet was resolved: that packet is lost.
        if let Some(old) = self.sent.get_mut(sequence.wrapping_sub(PACKET_WINDOW as u16)) {
            old.resolved = true;
        }
        self.sent.insert(sequence, record);
        let mut packet = Vec::with_capacity(MAX_PAYLOAD + 1);
        if whole.len() < MAX_PAYLOAD {
            packet.push(WHOLE);
            packet.extend_from_slice(&whole);
            self.emit(&packet, out);
            return;
        }
        let chunk = MAX_PAYLOAD - FRAGMENT_HEADER;
        let count = whole.len().div_ceil(chunk);
        for (index, part) in whole.chunks(chunk).enumerate() {
            packet.clear();
            packet.push(FRAGMENT);
            packet.extend_from_slice(&sequence.to_le_bytes());
            packet.push(index as u8);
            packet.push(count as u8);
            packet.extend_from_slice(part);
            self.emit(&packet, out);
        }
    }

    fn emit(&mut self, packet: &[u8], out: &mut dyn FnMut(&[u8])) {
        self.budget -= packet.len() as f64;
        self.stats.sent_bytes_per_second += packet.len() as f64;
        self.stats.packets_sent += 1;
        out(packet);
    }

    /// The latest packet received, and which of the 32 before it were.
    fn acks(&self) -> (u16, u32) {
        let Some(latest) = self.latest else { return (u16::MAX, 0) };
        let mut bits = 0u32;
        for i in 0..32u16 {
            if self.received.contains(latest.wrapping_sub(i + 1)) {
                bits |= 1 << i;
            }
        }
        (latest, bits)
    }
}

/// How many bytes the budget may save up: a quarter of a second's, and at
/// least a few packets.
fn burst(config: &ConnectionConfig) -> f64 {
    (config.bytes_per_second * 0.25).max(4.0 * MAX_PAYLOAD as f64)
}

pub(crate) fn write_varint(out: &mut Vec<u8>, mut value: u64) {
    loop {
        let byte = (value & 0x7F) as u8;
        value >>= 7;
        out.push(byte | if value != 0 { 0x80 } else { 0 });
        if value == 0 {
            return;
        }
    }
}

fn varint_len(value: u64) -> usize {
    (64 - value.leading_zeros() as usize).max(1).div_ceil(7)
}

/// Reads a packet's bytes, failing on any read past its end.
struct Bytes<'a>(&'a [u8]);

impl<'a> Bytes<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], PacketError> {
        if n > self.0.len() {
            return Err(PacketError::Malformed);
        }
        let (head, rest) = self.0.split_at(n);
        self.0 = rest;
        Ok(head)
    }

    fn u8(&mut self) -> Result<u8, PacketError> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16, PacketError> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().map_err(|_| PacketError::Malformed)?))
    }

    fn u32(&mut self) -> Result<u32, PacketError> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().map_err(|_| PacketError::Malformed)?))
    }

    fn varint(&mut self) -> Result<u64, PacketError> {
        let mut value = 0u64;
        for group in 0..10 {
            let byte = self.u8()?;
            value |= u64::from(byte & 0x7F) << (group * 7);
            if byte & 0x80 == 0 {
                return Ok(value);
            }
        }
        Err(PacketError::Malformed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tiny random number source, the same every run.
    pub(crate) struct Dice(u64);

    impl Dice {
        pub(crate) fn roll(&mut self) -> f64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            (self.0 >> 11) as f64 / (1u64 << 53) as f64
        }
    }

    /// The wire between two connections: it loses `loss` of packets and
    /// delivers the rest late by up to `jitter`, out of order, sometimes
    /// twice. Packets still in flight carry over from one run to the next.
    pub(crate) struct Wire {
        dice: Dice,
        flying: Vec<(f64, bool, Vec<u8>)>,
        loss: f64,
        jitter: f64,
    }

    impl Wire {
        pub(crate) fn new(loss: f64, jitter: f64) -> Self {
            Self { dice: Dice(0x2545F4914F6CDD1D), flying: Vec::new(), loss, jitter }
        }

        /// Runs both ends for `seconds` from `from`, flushing every 60th of a
        /// second; gives back the time it got to.
        pub(crate) fn run(&mut self, a: &mut Connection, b: &mut Connection, from: f64, seconds: f64) -> f64 {
            let tick = 1.0 / 60.0;
            let mut now = from;
            while now < from + seconds - 1e-9 {
                now += tick;
                for (to_b, conn) in [(true, &mut *a), (false, &mut *b)] {
                    let (dice, flying, loss, jitter) = (&mut self.dice, &mut self.flying, self.loss, self.jitter);
                    conn.flush(now, &mut |packet| {
                        if dice.roll() >= loss {
                            let at = now + 0.02 + dice.roll() * jitter;
                            flying.push((at, to_b, packet.to_vec()));
                            if dice.roll() < 0.05 {
                                flying.push((at + 0.01, to_b, packet.to_vec()));
                            }
                        }
                    });
                }
                let (due, later): (Vec<_>, Vec<_>) = self.flying.drain(..).partition(|(at, ..)| *at <= now);
                self.flying = later;
                for (_, to_b, packet) in due {
                    let conn = if to_b { &mut *b } else { &mut *a };
                    let _ = conn.receive_packet(&packet, now);
                }
            }
            now
        }
    }

    #[test]
    fn reliable_messages_arrive_once_and_in_order_through_loss() {
        let mut a = Connection::new(ConnectionConfig::default(), 0.0);
        let mut b = Connection::new(ConnectionConfig::default(), 0.0);
        let big: Vec<u8> = (0..5000u32).map(|i| (i * 7) as u8).collect();
        for i in 0..300u32 {
            a.send(Channel::RELIABLE, &i.to_le_bytes()).unwrap();
            if i == 150 {
                a.send(Channel::RELIABLE, &big).unwrap();
            }
        }
        Wire::new(0.2, 0.08).run(&mut a, &mut b, 0.0, 6.0);
        let mut got = Vec::new();
        while let Some(message) = b.receive(Channel::RELIABLE) {
            got.push(message);
        }
        assert_eq!(got.len(), 301, "every message, the large one put back together");
        let numbers: Vec<u32> =
            got.iter().filter(|m| m.len() == 4).map(|m| u32::from_le_bytes(m[..].try_into().unwrap())).collect();
        assert_eq!(numbers, (0..300).collect::<Vec<_>>(), "in order");
        assert_eq!(got[151], big);
        assert!(a.channels[1].pending.is_empty() && a.channels[1].memory == 0, "all acknowledged");
    }

    #[test]
    fn unordered_messages_arrive_once_each() {
        let mut a = Connection::new(ConnectionConfig::default(), 0.0);
        let mut b = Connection::new(ConnectionConfig::default(), 0.0);
        for i in 0..200u32 {
            a.send(Channel::RELIABLE_UNORDERED, &i.to_le_bytes()).unwrap();
        }
        Wire::new(0.2, 0.08).run(&mut a, &mut b, 0.0, 5.0);
        let mut got: Vec<u32> = std::iter::from_fn(|| b.receive(Channel::RELIABLE_UNORDERED))
            .map(|m| u32::from_le_bytes(m[..].try_into().unwrap()))
            .collect();
        got.sort();
        assert_eq!(got, (0..200).collect::<Vec<_>>());
    }

    #[test]
    fn a_tagged_message_reports_its_packet_acknowledged_or_lost() {
        let mut a = Connection::new(ConnectionConfig::default(), 0.0);
        let mut b = Connection::new(ConnectionConfig::default(), 0.0);
        let mut wire = Wire::new(0.3, 0.02);
        let mut now = 0.0;
        let (mut acked, mut lost) = (0, 0);
        for tick in 0..400u32 {
            a.send_tagged(Channel::UNRELIABLE, &[1, 2, 3], tick).unwrap();
            now = wire.run(&mut a, &mut b, now, 1.0 / 60.0);
            for delivery in a.take_deliveries() {
                match delivery {
                    Delivery::Acked(_) => acked += 1,
                    Delivery::Lost(_) => lost += 1,
                }
            }
        }
        let share = f64::from(lost) / f64::from(acked + lost);
        assert!(acked + lost > 360, "nearly all resolved: {acked} acked, {lost} lost");
        assert!((0.2..0.4).contains(&share), "about the loss: {share}");
        assert!(a.stats().rtt > 0.02 && a.stats().rtt < 0.2, "round trip {}", a.stats().rtt);
    }

    #[test]
    fn a_large_unreliable_message_goes_in_fragments() {
        let mut a = Connection::new(ConnectionConfig::default(), 0.0);
        let mut b = Connection::new(ConnectionConfig::default(), 0.0);
        let snapshot = vec![9u8; 3000];
        a.send(Channel::UNRELIABLE, &snapshot).unwrap();
        let mut packets = Vec::new();
        a.flush(0.0, &mut |p| packets.push(p.to_vec()));
        assert_eq!(packets.len(), 3);
        assert!(packets.iter().all(|p| p.len() <= MAX_PAYLOAD));
        for p in packets.iter().rev() {
            b.receive_packet(p, 0.01).unwrap();
        }
        assert_eq!(b.receive(Channel::UNRELIABLE), Some(snapshot));
        assert_eq!(a.send(Channel::UNRELIABLE, &vec![0; 5000]), Err(SendError::TooLarge));
    }

    #[test]
    fn the_budget_holds_what_does_not_fit_until_later() {
        let config = ConnectionConfig { bytes_per_second: 12_000.0, ..ConnectionConfig::default() };
        let mut a = Connection::new(config, 0.0);
        for _ in 0..100 {
            a.send(Channel::RELIABLE, &[0; 1000]).unwrap();
        }
        let mut sent = 0;
        let mut now = 0.0;
        for _ in 0..60 {
            now += 1.0 / 60.0;
            a.flush(now, &mut |p| sent += p.len());
        }
        // A second at 12 kB/s, with the quarter-second's head start it may save up.
        assert!(sent <= 12_000 + 6_000, "kept to its budget: {sent}");
        assert!(sent >= 10_000, "but used it: {sent}");
    }

    #[test]
    fn garbled_packets_are_turned_away_without_harm() {
        let mut b = Connection::new(ConnectionConfig::default(), 0.0);
        let mut dice = Dice(7);
        for length in 0..200 {
            let packet: Vec<u8> = (0..length).map(|_| (dice.roll() * 256.0) as u8).collect();
            let _ = b.receive_packet(&packet, 0.0);
        }
        assert_eq!(b.receive_packet(&[WHOLE, 0, 0, 0], 0.0), Err(PacketError::Malformed));
        assert_eq!(b.receive_packet(&[FRAGMENT, 0, 0, 5, 9], 0.0), Err(PacketError::Malformed));
    }

    #[test]
    fn a_full_channel_fails_the_connection() {
        let config = ConnectionConfig {
            channels: vec![ChannelConfig { kind: ChannelKind::ReliableOrdered, max_memory: 2000 }],
            ..ConnectionConfig::default()
        };
        let mut a = Connection::new(config, 0.0);
        a.send(Channel(0), &[0; 1500]).unwrap();
        assert_eq!(a.send(Channel(0), &[0; 1500]), Err(SendError::ChannelFull));
        assert_eq!(a.error(), Some(SendError::ChannelFull));
        assert_eq!(a.send(Channel(9), &[0]), Err(SendError::NoChannel));
    }
}
