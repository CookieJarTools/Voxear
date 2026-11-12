//! What a map knows of a world: kept apart from the world itself, so a
//! game can give each player a map of their own that shows what that
//! player has seen and heard, however the world is now.
//!
//! A [`MapKnowledge`] holds the ground seen, a bit for each square of
//! [`KNOWN_CELL`] metres, and news: layers a game makes for itself (how
//! dangerous a place is, who holds it), each a value for each square of
//! [`NEWS_CELL`] metres with when it was heard. Nothing here reads the
//! world: a game writes what a player learns, when they learn it, and the
//! map shows that and no more.
//!
//! Places are in metres from the world of blocks' corner, along x and z.
//! Knowledge is kept by region, [`REGION`] metres a side, and is small:
//! written out (see [`MapKnowledge::to_bytes`]) a region is runs of seen
//! and unseen and of like news, compressed. Two players' are joined with
//! [`MapKnowledge::merge`], the newer news winning, and what changed since
//! some time is written alone (see [`MapKnowledge::changes_since`]) to send.

use std::collections::BTreeMap;

use anyhow::{Result, ensure};

/// How many metres a side each square of ground seen is.
pub const KNOWN_CELL: f32 = 8.0;
/// How many metres a side each square of news is.
pub const NEWS_CELL: f32 = 32.0;
/// How many metres a side a region is: 64 squares of ground seen, 16 of
/// news.
pub const REGION: f32 = 512.0;

const SEEN_SIDE: i32 = 64;
const NEWS_SIDE: i32 = 16;
const SEEN_CELLS: usize = (SEEN_SIDE * SEEN_SIDE) as usize;
const NEWS_CELLS: usize = (NEWS_SIDE * NEWS_SIDE) as usize;

const MAGIC: &[u8; 4] = b"VXMK";
const VERSION: u8 = 1;

/// One layer of news in a region: each square's value, 0 for none heard,
/// and when each was heard.
#[derive(Clone, Debug, PartialEq, Eq)]
struct News {
    values: Vec<u8>,
    heard: Vec<u32>,
}

impl News {
    fn new() -> Self {
        News { values: vec![0; NEWS_CELLS], heard: vec![0; NEWS_CELLS] }
    }
}

/// A region's knowledge: its ground seen, a bit a square, its news by
/// layer, and the stamp it last changed at.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Region {
    seen: Vec<u64>,
    news: BTreeMap<u8, News>,
    changed: u32,
}

impl Region {
    fn new() -> Self {
        Region { seen: vec![0; SEEN_CELLS / 64], news: BTreeMap::new(), changed: 0 }
    }

    fn is_empty(&self) -> bool {
        self.seen.iter().all(|bits| *bits == 0) && self.news.is_empty()
    }
}

/// What one map knows of a world. See the module's own words.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MapKnowledge {
    regions: BTreeMap<[i32; 2], Region>,
    /// Whether all the ground is seen, whatever the regions say.
    all: bool,
    /// Counted up with each change, so what changed since a stamp is known.
    stamp: u32,
}

/// The region a square lies in, and where in it, for squares `side` to a
/// region.
fn within(cell: [i32; 2], side: i32) -> ([i32; 2], usize) {
    let region = cell.map(|at| at.div_euclid(side));
    let local = cell.map(|at| at.rem_euclid(side));
    (region, (local[1] * side + local[0]) as usize)
}

/// The squares `size` metres a side whose middles lie within `radius`
/// metres of `[x, z]`; the one it is in, at least.
fn squares(x: f32, z: f32, radius: f32, size: f32) -> impl Iterator<Item = [i32; 2]> {
    let radius = radius.max(0.0);
    let low = [x - radius, z - radius].map(|at| (at / size).floor() as i32);
    let high = [x + radius, z + radius].map(|at| (at / size).floor() as i32);
    let own = [x, z].map(|at| (at / size).floor() as i32);
    let near = move |cell: &[i32; 2]| {
        let middle = cell.map(|at| (at as f32 + 0.5) * size);
        (middle[0] - x).hypot(middle[1] - z) <= radius || *cell == own
    };
    (low[1]..=high[1]).flat_map(move |cz| (low[0]..=high[0]).map(move |cx| [cx, cz])).filter(near)
}

impl MapKnowledge {
    /// Knowledge of nothing.
    pub fn new() -> Self {
        Self::default()
    }

    /// Counted up with each change: what [`MapKnowledge::changes_since`]
    /// takes.
    pub fn stamp(&self) -> u32 {
        self.stamp
    }

    /// Whether nothing is known.
    pub fn is_empty(&self) -> bool {
        !self.all && self.regions.is_empty()
    }

    fn touch(&mut self, region: [i32; 2]) -> &mut Region {
        let kept = self.regions.entry(region).or_insert_with(Region::new);
        kept.changed = self.stamp;
        kept
    }

    /// Sees the ground within `radius` metres of `[x, z]`. Whether any of
    /// it was not seen before.
    pub fn reveal(&mut self, x: f32, z: f32, radius: f32) -> bool {
        let unseen: Vec<[i32; 2]> = squares(x, z, radius, KNOWN_CELL).filter(|cell| !self.sees(*cell)).collect();
        if unseen.is_empty() {
            return false;
        }
        self.stamp += 1;
        for cell in unseen {
            let (region, at) = within(cell, SEEN_SIDE);
            self.touch(region).seen[at / 64] |= 1 << (at % 64);
        }
        true
    }

    /// Sees all the ground there is, or stops doing so: what was seen
    /// square by square is kept either way.
    pub fn reveal_all(&mut self, all: bool) {
        if self.all != all {
            self.stamp += 1;
            self.all = all;
        }
    }

    /// Whether all the ground is seen (see [`MapKnowledge::reveal_all`]).
    pub fn knows_all(&self) -> bool {
        self.all
    }

    /// Forgets everything: the ground seen and all news.
    pub fn forget(&mut self) {
        if !self.is_empty() {
            *self = MapKnowledge { stamp: self.stamp + 1, ..MapKnowledge::default() };
        }
    }

    fn sees(&self, cell: [i32; 2]) -> bool {
        let (region, at) = within(cell, SEEN_SIDE);
        self.regions.get(&region).is_some_and(|kept| kept.seen[at / 64] >> (at % 64) & 1 == 1)
    }

    /// Whether the ground at `[x, z]` is seen.
    pub fn knows(&self, x: f32, z: f32) -> bool {
        self.all || self.sees([x, z].map(|at| (at / KNOWN_CELL).floor() as i32))
    }

    /// Hears, at time `heard`, that layer `layer` is `value`, from 0 to 1,
    /// within `radius` metres of `[x, z]`: in place of whatever was known
    /// there. The time is the game's own, in any unit that only grows.
    pub fn paint(&mut self, layer: u8, [x, z]: [f32; 2], radius: f32, value: f32, heard: u32) {
        // Kept from 1 to 255, so no news is told from news of nothing.
        let value = 1 + (value.clamp(0.0, 1.0) * 254.0).round() as u8;
        self.stamp += 1;
        for cell in squares(x, z, radius, NEWS_CELL) {
            let (region, at) = within(cell, NEWS_SIDE);
            let news = self.touch(region).news.entry(layer).or_insert_with(News::new);
            (news.values[at], news.heard[at]) = (value, heard);
        }
    }

    /// What was last heard of layer `layer` at `[x, z]`: its value, from 0
    /// to 1, and when it was heard. None where nothing was.
    pub fn news(&self, layer: u8, x: f32, z: f32) -> Option<(f32, u32)> {
        let (region, at) = within([x, z].map(|at| (at / NEWS_CELL).floor() as i32), NEWS_SIDE);
        let news = self.regions.get(&region)?.news.get(&layer)?;
        (news.values[at] != 0).then(|| (f32::from(news.values[at] - 1) / 254.0, news.heard[at]))
    }

    /// The latest stamp of any change within the square from `low` to
    /// `high`, in metres: 0 where nothing was ever known. All the ground
    /// seen, or no longer, changes everywhere.
    pub fn changed_within(&self, low: [f32; 2], high: [f32; 2]) -> u32 {
        let first = low.map(|at| (at / REGION).floor() as i32);
        let last = high.map(|at| (at / REGION).floor() as i32);
        let regions = (first[1]..=last[1]).flat_map(|z| (first[0]..=last[0]).map(move |x| [x, z]));
        let latest = regions.filter_map(|region| self.regions.get(&region)).map(|kept| kept.changed).max();
        latest.unwrap_or(0) + if self.all { 1 << 31 } else { 0 }
    }

    /// Takes in what `other` knows: ground either has seen is seen, and of
    /// two pieces of news of a place the one heard later is kept.
    pub fn merge(&mut self, other: &MapKnowledge) {
        self.stamp += 1;
        self.all |= other.all;
        for (key, theirs) in &other.regions {
            let ours = self.touch(*key);
            for (kept, seen) in ours.seen.iter_mut().zip(&theirs.seen) {
                *kept |= seen;
            }
            for (layer, news) in &theirs.news {
                let kept = ours.news.entry(*layer).or_insert_with(News::new);
                for at in 0..NEWS_CELLS {
                    if news.values[at] != 0 && (kept.values[at] == 0 || news.heard[at] > kept.heard[at]) {
                        (kept.values[at], kept.heard[at]) = (news.values[at], news.heard[at]);
                    }
                }
            }
        }
    }

    /// All of it as a few bytes, to save or to send: read back with
    /// [`MapKnowledge::from_bytes`].
    pub fn to_bytes(&self) -> Vec<u8> {
        self.written(0)
    }

    /// What changed after stamp `since`, as bytes another's knowledge takes
    /// in with [`MapKnowledge::merge`] once read: for sending a player what
    /// they just learnt, not their whole map.
    pub fn changes_since(&self, since: u32) -> Vec<u8> {
        self.written(since)
    }

    fn written(&self, since: u32) -> Vec<u8> {
        let regions: Vec<(&[i32; 2], &Region)> =
            self.regions.iter().filter(|(_, kept)| since == 0 || kept.changed > since).collect();
        let mut bytes = Vec::with_capacity(16 + regions.len() * 64);
        bytes.extend(MAGIC);
        bytes.push(VERSION);
        bytes.push(u8::from(self.all));
        bytes.extend((regions.len() as u32).to_le_bytes());
        for (key, region) in regions {
            bytes.extend(key[0].to_le_bytes());
            bytes.extend(key[1].to_le_bytes());
            write_seen(&mut bytes, &region.seen);
            bytes.push(region.news.len() as u8);
            for (layer, news) in &region.news {
                bytes.push(*layer);
                write_news(&mut bytes, news);
            }
        }
        lz4_flex::compress_prepend_size(&bytes)
    }

    /// Knowledge from what [`MapKnowledge::to_bytes`] or
    /// [`MapKnowledge::changes_since`] gave.
    pub fn from_bytes(compressed: &[u8]) -> Result<Self> {
        let bytes = lz4_flex::decompress_size_prepended(compressed)?;
        let mut read = Reader { bytes: &bytes, at: 0 };
        ensure!(read.take(4)? == MAGIC && read.u8()? == VERSION, "not a map's knowledge this version reads");
        let mut known = MapKnowledge { all: read.u8()? != 0, stamp: 1, ..MapKnowledge::default() };
        for _ in 0..read.u32()? {
            let key = [read.u32()? as i32, read.u32()? as i32];
            let mut region = Region { changed: 1, ..Region::new() };
            read_seen(&mut read, &mut region.seen)?;
            for _ in 0..read.u8()? {
                let layer = read.u8()?;
                region.news.insert(layer, read_news(&mut read)?);
            }
            if !region.is_empty() {
                known.regions.insert(key, region);
            }
        }
        ensure!(read.at == bytes.len(), "a map's knowledge with more after it");
        Ok(known)
    }
}

/// A region's seen squares as runs: unseen first, then seen, turn about,
/// each a count of squares in two bytes.
fn write_seen(bytes: &mut Vec<u8>, seen: &[u64]) {
    let bit = |at: usize| seen[at / 64] >> (at % 64) & 1 == 1;
    let mut runs: Vec<u16> = Vec::new();
    let (mut now, mut run) = (false, 0u16);
    for at in 0..SEEN_CELLS {
        if bit(at) != now {
            runs.push(run);
            (now, run) = (!now, 0);
        }
        run += 1;
    }
    runs.push(run);
    bytes.extend((runs.len() as u16).to_le_bytes());
    runs.iter().for_each(|run| bytes.extend(run.to_le_bytes()));
}

fn read_seen(read: &mut Reader, seen: &mut [u64]) -> Result<()> {
    let (mut at, mut now) = (0usize, false);
    for _ in 0..read.u16()? {
        let run = usize::from(read.u16()?);
        ensure!(at + run <= SEEN_CELLS, "a region with more squares than it has");
        if now {
            (at..at + run).for_each(|cell| seen[cell / 64] |= 1 << (cell % 64));
        }
        (at, now) = (at + run, !now);
    }
    Ok(())
}

/// A layer's news as runs of squares with the same value heard at the same
/// time: a count, the value, the time.
fn write_news(bytes: &mut Vec<u8>, news: &News) {
    let mut runs: Vec<(u16, u8, u32)> = Vec::new();
    for at in 0..NEWS_CELLS {
        match runs.last_mut() {
            Some((count, value, heard)) if (*value, *heard) == (news.values[at], news.heard[at]) => *count += 1,
            _ => runs.push((1, news.values[at], news.heard[at])),
        }
    }
    bytes.extend((runs.len() as u16).to_le_bytes());
    for (count, value, heard) in runs {
        bytes.extend(count.to_le_bytes());
        bytes.push(value);
        bytes.extend(heard.to_le_bytes());
    }
}

fn read_news(read: &mut Reader) -> Result<News> {
    let mut news = News::new();
    let mut at = 0usize;
    for _ in 0..read.u16()? {
        let (count, value, heard) = (usize::from(read.u16()?), read.u8()?, read.u32()?);
        ensure!(at + count <= NEWS_CELLS, "a layer with more squares than it has");
        news.values[at..at + count].fill(value);
        news.heard[at..at + count].fill(heard);
        at += count;
    }
    Ok(news)
}

/// Reads bytes in order.
struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn take(&mut self, count: usize) -> Result<&[u8]> {
        ensure!(self.at + count <= self.bytes.len(), "a map's knowledge cut short");
        self.at += count;
        Ok(&self.bytes[self.at - count..self.at])
    }

    fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into()?))
    }

    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into()?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const THREAT: u8 = 1;
    const OWNER: u8 = 2;

    #[test]
    fn ground_is_seen_round_where_it_was_revealed_and_nowhere_else() {
        let mut known = MapKnowledge::new();
        assert!(known.is_empty() && !known.knows(0.0, 0.0));
        assert!(known.reveal(100.0, -40.0, 30.0), "new ground");
        assert!(known.knows(100.0, -40.0) && known.knows(120.0, -40.0) && known.knows(100.0, -60.0));
        assert!(!known.knows(140.0, -40.0) && !known.knows(100.0, 0.0), "past its radius");
        assert!(!known.reveal(100.0, -40.0, 30.0), "nothing new the second time");
        let stamp = known.stamp();
        known.reveal(100.0, -40.0, 10.0);
        assert_eq!(known.stamp(), stamp, "nothing changed, so nothing to send");
        // A radius of nothing still sees the square stood in.
        assert!(known.reveal(-700.0, 900.0, 0.0) && known.knows(-700.0, 900.0));
        // All the ground, and back: what was seen is kept.
        known.reveal_all(true);
        assert!(known.knows(5000.0, 5000.0));
        known.reveal_all(false);
        assert!(!known.knows(5000.0, 5000.0) && known.knows(100.0, -40.0));
        known.forget();
        assert!(known.is_empty() && !known.knows(100.0, -40.0));
    }

    #[test]
    fn news_is_what_was_last_heard_of_a_place_with_when_and_is_not_the_ground_seen() {
        let mut known = MapKnowledge::new();
        assert_eq!(known.news(THREAT, 0.0, 0.0), None);
        known.paint(THREAT, [200.0, 200.0], 80.0, 0.5, 10);
        let (value, heard) = known.news(THREAT, 200.0, 200.0).unwrap();
        assert!((value - 0.5).abs() < 0.01 && heard == 10);
        assert!(known.news(THREAT, 250.0, 200.0).is_some() && known.news(THREAT, 320.0, 200.0).is_none());
        assert_eq!(known.news(OWNER, 200.0, 200.0), None, "another layer heard nothing");
        assert!(!known.knows(200.0, 200.0), "hearing of a place is not seeing it");
        // Heard again, it is what was heard last: here, that the danger is gone.
        known.paint(THREAT, [200.0, 200.0], 0.0, 0.0, 25);
        assert_eq!(known.news(THREAT, 200.0, 200.0), Some((0.0, 25)));
        assert_eq!(known.news(THREAT, 250.0, 200.0).unwrap().1, 10, "a square beside it keeps its older news");
    }

    #[test]
    fn two_maps_join_with_all_either_saw_and_the_newer_news() {
        let (mut mine, mut yours) = (MapKnowledge::new(), MapKnowledge::new());
        mine.reveal(0.0, 0.0, 40.0);
        mine.paint(THREAT, [1000.0, 0.0], 0.0, 0.2, 5);
        mine.paint(OWNER, [0.0, 0.0], 0.0, 1.0, 5);
        yours.reveal(2000.0, 0.0, 40.0);
        yours.paint(THREAT, [1000.0, 0.0], 0.0, 0.9, 9);
        yours.paint(OWNER, [0.0, 0.0], 0.0, 0.0, 2);
        mine.merge(&yours);
        assert!(mine.knows(0.0, 0.0) && mine.knows(2000.0, 0.0) && !mine.knows(1000.0, 0.0));
        assert!((mine.news(THREAT, 1000.0, 0.0).unwrap().0 - 0.9).abs() < 0.01, "yours is newer");
        assert_eq!(mine.news(OWNER, 0.0, 0.0), Some((1.0, 5)), "mine is newer");
        assert!(!yours.knows(0.0, 0.0), "the one taken from is as it was");
    }

    #[test]
    fn knowledge_is_written_small_read_back_the_same_and_its_changes_sent_alone() {
        let mut known = MapKnowledge::new();
        // A walk of four kilometres, seeing a hundred metres round, and news of a dozen places.
        for step in 0..400 {
            known.reveal(step as f32 * 10.0, (step as f32 * 0.05).sin() * 300.0, 100.0);
        }
        for place in 0..12 {
            known.paint(THREAT, [place as f32 * 700.0, 150.0], 120.0, place as f32 / 12.0, 100 + place);
        }
        let bytes = known.to_bytes();
        let read = MapKnowledge::from_bytes(&bytes).unwrap();
        for at in [[0.0, 0.0], [1500.0, 90.0], [3990.0, -200.0], [900.0, 2000.0]] {
            assert_eq!(read.knows(at[0], at[1]), known.knows(at[0], at[1]), "{at:?}");
            assert_eq!(read.news(THREAT, at[0], at[1]), known.news(THREAT, at[0], at[1]), "{at:?}");
        }
        println!("four kilometres walked and twelve places heard of: {} bytes", bytes.len());
        assert!(bytes.len() < 4096, "a long walk's map in one message: {} bytes", bytes.len());
        assert!(MapKnowledge::from_bytes(&bytes[..bytes.len() / 2]).is_err());
        assert!(MapKnowledge::from_bytes(&MapKnowledge::new().to_bytes()).unwrap().is_empty());

        // What a player learns at a board is sent alone, and joins the map of who is sent it.
        let before = known.stamp();
        known.paint(THREAT, [3000.0, 150.0], 60.0, 1.0, 500);
        let news = known.changes_since(before);
        assert!(news.len() < bytes.len() / 2, "{} of {} bytes", news.len(), bytes.len());
        let mut other = MapKnowledge::new();
        other.merge(&MapKnowledge::from_bytes(&news).unwrap());
        assert_eq!(other.news(THREAT, 3000.0, 150.0), Some((1.0, 500)));
        assert!(!other.knows(0.0, 0.0), "only what changed went: the region of the news, not the whole walk");
        assert!(MapKnowledge::from_bytes(&known.changes_since(known.stamp())).unwrap().is_empty());
    }

    #[test]
    fn what_changed_within_a_square_is_known_by_its_stamp() {
        let mut known = MapKnowledge::new();
        assert_eq!(known.changed_within([0.0, 0.0], [256.0, 256.0]), 0);
        known.reveal(100.0, 100.0, 20.0);
        let near = known.changed_within([0.0, 0.0], [256.0, 256.0]);
        assert!(near > 0);
        assert_eq!(known.changed_within([4000.0, 0.0], [4256.0, 256.0]), 0, "far from what was seen");
        known.paint(THREAT, [4100.0, 100.0], 0.0, 1.0, 1);
        assert!(known.changed_within([4000.0, 0.0], [4256.0, 256.0]) > near);
        assert_eq!(known.changed_within([0.0, 0.0], [256.0, 256.0]), near, "untouched since");
        known.reveal_all(true);
        assert_ne!(known.changed_within([0.0, 0.0], [256.0, 256.0]), near, "seeing all changes everywhere");
    }
}
