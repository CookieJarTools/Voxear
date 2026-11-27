//! What players ask of the server's endless Voxel Volumes, on the server:
//! each request ([`VoxelAsk`]) is checked before it is made, the easy way
//! by the game's rules ([`VoxelAskRules`]: which kinds players may ask, how
//! far from their own Obj, how large), the full way by a check of the
//! game's own, in Rust ([`set_ask_check`]) or in C# (`VoxelRequests.Check`),
//! which sees each request whole, with who asked, and accepts or refuses it
//! with a reason. A request a C# check holds is answered within the frame
//! it came in, before the server sends what came of it; one it never
//! answers is refused after a second. Every request is answered, so the
//! player's machine lets go of what it showed of it, and the last refusals
//! are kept for `vox editor call net.voxels`.

use std::collections::VecDeque;
use std::sync::Arc;

use voxear_core::math::Vec3;
use voxear_ecs::World;
use voxear_render::{VoxelCut, VoxelMaterial};

use crate::net::{self, PlayerId};
use crate::voxel_sync::VoxelAsk;

/// A kind of request, as rules and checks name them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum VoxelAskKind {
    Dig,
    Place,
    Restore,
    /// A shape carved out: a ball, box, bank, area or cover of air.
    Carve,
    /// A shape filled with a ground or paving.
    Fill,
    Print,
    Cut,
    Blast,
    /// A rope tied.
    Hang,
    CutRope,
}

impl VoxelAskKind {
    pub const ALL: [Self; 10] = [
        Self::Dig,
        Self::Place,
        Self::Restore,
        Self::Carve,
        Self::Fill,
        Self::Print,
        Self::Cut,
        Self::Blast,
        Self::Hang,
        Self::CutRope,
    ];

    /// Its bit among [`VoxelAskRules::allowed`], as `VoxelRequestKinds` in
    /// C# has it.
    pub fn bit(self) -> u32 {
        1 << Self::ALL.iter().position(|kind| *kind == self).unwrap_or(0)
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Dig => "dig",
            Self::Place => "place",
            Self::Restore => "restore",
            Self::Carve => "carve",
            Self::Fill => "fill",
            Self::Print => "print",
            Self::Cut => "cut",
            Self::Blast => "blast",
            Self::Hang => "hang",
            Self::CutRope => "cut_rope",
        }
    }
}

/// What players may ask of the volumes, the easy way: the kinds allowed (a
/// bit each, [`VoxelAskKind::bit`]), how far from their own first Obj in
/// metres (0 for anywhere), how large a ball's radius or a box's side may
/// be, and how many points at once.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VoxelAskRules {
    pub allowed: u32,
    pub reach: f32,
    pub largest: f32,
    pub most_points: usize,
}

impl Default for VoxelAskRules {
    fn default() -> Self {
        Self { allowed: u32::MAX, reach: 0.0, largest: crate::voxel_sync::MOST_ASKED_REACH, most_points: 4096 }
    }
}

/// A request as a check sees it: its kind, where it is in the world, how
/// large (a radius or a box's largest side, in metres), how many points,
/// and the ground it fills or places, if any.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VoxelAskInfo {
    pub kind: VoxelAskKind,
    pub at: Vec3,
    pub size: f32,
    pub points: usize,
    pub material: Option<VoxelMaterial>,
}

impl VoxelAsk {
    /// What it comes to, for a check.
    pub fn info(&self) -> VoxelAskInfo {
        let middle = |points: &[Vec3]| points.iter().fold(Vec3::ZERO, |sum, p| sum + *p) / points.len().max(1) as f32;
        let shaped = |data: &crate::voxel_volumes::VoxelEditData| {
            let size = data.size.iter().copied().fold(data.radius, f32::max);
            let at = match data.points.is_empty() {
                true => Vec3::from(data.at),
                false => {
                    let n = data.points.len() as f32;
                    let (x, z) = data.points.iter().fold((0.0, 0.0), |(x, z), p| (x + p[0], z + p[1]));
                    Vec3::new(x / n, (data.low + data.high) * 0.5, z / n)
                }
            };
            (at, size, data.points.len())
        };
        let (kind, at, size, points, material) = match self {
            Self::Shape(data) => {
                let (at, size, points) = shaped(data);
                let kind = if data.fill.is_some() { VoxelAskKind::Fill } else { VoxelAskKind::Carve };
                (kind, at, size, points, data.fill)
            }
            Self::Print(data) => {
                let (at, size, points) = shaped(data);
                (VoxelAskKind::Print, at, size.min(1.0), points, data.fill)
            }
            Self::Set { points, to } => {
                let kind = if to.is_some() { VoxelAskKind::Place } else { VoxelAskKind::Dig };
                (kind, middle(points), 0.0, points.len(), to.map(|kind| kind.material))
            }
            Self::Restore { points } => (VoxelAskKind::Restore, middle(points), 0.0, points.len(), None),
            Self::Cut { cut, .. } => match cut {
                VoxelCut::Points(points) => (VoxelAskKind::Cut, middle(points), 0.0, points.len(), None),
                VoxelCut::Ball { at, radius } | VoxelCut::Slice { at, radius, .. } => {
                    (VoxelAskKind::Cut, *at, *radius, 0, None)
                }
            },
            Self::Blast { at, radius, .. } => (VoxelAskKind::Blast, *at, *radius, 0, None),
            Self::Hang { tied, holds, .. } => (VoxelAskKind::Hang, *tied, tied.distance(*holds), 0, None),
            Self::CutRope { at, .. } => (VoxelAskKind::CutRope, *at, 0.0, 0, None),
        };
        VoxelAskInfo { kind, at, size, points, material }
    }
}

/// The game's own check in Rust: true to make the request.
pub type AskCheck = Arc<dyn Fn(&World, PlayerId, &VoxelAsk) -> bool + Send + Sync>;

/// A request refused, for the stats: who asked, its number from them, its
/// kind, why, and the server's tick.
#[derive(Clone, Debug, PartialEq)]
pub struct Refusal {
    pub player: PlayerId,
    pub number: u32,
    pub kind: VoxelAskKind,
    pub reason: String,
    pub tick: u64,
}

/// How many refusals are kept.
const KEPT_REFUSALS: usize = 32;
/// Frames a request waits for the game's C# check before it is refused.
const SCRIPT_WAIT: u32 = 60;

/// A request waiting for the game's C# check.
#[derive(Clone, Debug)]
struct Held {
    id: u32,
    player: PlayerId,
    number: u32,
    ask: VoxelAsk,
    frames: u32,
    answer: Option<(bool, String)>,
    told: bool,
}

/// The server's requests: the rules, the checks, what waits and what was
/// refused.
#[derive(Default)]
pub(crate) struct Asks {
    rules: VoxelAskRules,
    check: Option<AskCheck>,
    scripts_check: bool,
    held: Vec<Held>,
    ready: Vec<(PlayerId, u32, VoxelAsk)>,
    refused: Vec<(PlayerId, u32)>,
    refusals: VecDeque<Refusal>,
    next_id: u32,
}

fn asks(world: &mut World) -> &mut Asks {
    if world.resource::<Asks>().is_none() {
        world.insert_resource(Asks::default());
    }
    world.resource_mut::<Asks>().expect("just made")
}

/// Sets what players may ask of the volumes, the easy way.
pub fn set_rules(world: &mut World, rules: VoxelAskRules) {
    asks(world).rules = rules;
}

/// What players may ask of the volumes, the easy way.
pub fn rules(world: &World) -> VoxelAskRules {
    world.resource::<Asks>().map(|asks| asks.rules).unwrap_or_default()
}

/// Has the server ask `check` of every request a player makes of its
/// volumes, after the rules, before making it.
pub fn set_ask_check(world: &mut World, check: AskCheck) {
    asks(world).check = Some(check);
}

/// Has the server hold each request for the game's C# check, or not.
pub fn set_scripts_check(world: &mut World, on: bool) {
    asks(world).scripts_check = on;
}

/// Why `ask` from `player` breaks `rules`: none where it keeps to them.
fn broken(world: &World, rules: &VoxelAskRules, player: PlayerId, ask: &VoxelAsk) -> Option<String> {
    let info = ask.info();
    if rules.allowed & info.kind.bit() == 0 {
        return Some(format!("players may not {}", info.kind.name()));
    }
    if !info.size.is_finite() || info.size > rules.largest {
        return Some(format!("larger than {} m", rules.largest));
    }
    if info.points > rules.most_points {
        return Some(format!("more than {} points", rules.most_points));
    }
    if rules.reach > 0.0 {
        let near = net::player_place(world, player).is_some_and(|at| at.distance(info.at) <= rules.reach);
        if !near {
            return Some(format!("further than {} m from their own Obj", rules.reach));
        }
    }
    if let Some(data) = match ask {
        VoxelAsk::Shape(data) | VoxelAsk::Print(data) => Some(data),
        _ => None,
    } && data.points.len() > 256
    {
        return Some("an outline of more than 256 points".into());
    }
    None
}

/// On a server, as a frame begins: takes in what players asked, refusing
/// what breaks the rules or the Rust check, holding the rest for the C#
/// check where the game has one, else readying it to be made.
pub(crate) fn receive(world: &mut World) {
    let asked = net::take_voxel_asks(world);
    if asked.is_empty() {
        return;
    }
    let origin = voxear_scene::WorldOrigin::of(world).0.as_vec3();
    let tick = world.resource::<net::NetSession>().map_or(0, |s| s.tick());
    let (rules, check, scripts) = {
        let asks = asks(world);
        (asks.rules, asks.check.clone(), asks.scripts_check)
    };
    for (player, bytes) in asked {
        let Some((number, ask)) = crate::voxel_sync::read_ask(&bytes) else {
            tracing::warn!("player {} asked something of the voxel volumes that could not be read", player.0);
            continue;
        };
        let ask = ask.moved(-origin);
        let refusal = broken(world, &rules, player, &ask)
            .or_else(|| check.as_ref().filter(|check| !check(world, player, &ask)).map(|_| "the game's check".into()));
        let asks = asks(world);
        match refusal {
            Some(reason) => refuse(asks, player, number, ask.info().kind, reason, tick),
            None if scripts => {
                asks.next_id = asks.next_id.wrapping_add(1).max(1);
                let id = asks.next_id;
                asks.held.push(Held { id, player, number, ask, frames: 0, answer: None, told: false });
            }
            None => asks.ready.push((player, number, ask)),
        }
    }
}

fn refuse(asks: &mut Asks, player: PlayerId, number: u32, kind: VoxelAskKind, reason: String, tick: u64) {
    asks.refused.push((player, number));
    asks.refusals.push_back(Refusal { player, number, kind, reason, tick });
    while asks.refusals.len() > KEPT_REFUSALS {
        asks.refusals.pop_front();
    }
}

/// On a server, before what it sends goes: the requests to make, in order,
/// those the C# check accepted among them; and those refused, each with who
/// asked and its number from them.
pub(crate) fn take(world: &mut World) -> (Vec<(PlayerId, u32, VoxelAsk)>, Vec<(PlayerId, u32)>) {
    let Some(asks) = world.resource_mut::<Asks>() else { return (Vec::new(), Vec::new()) };
    let tick = 0;
    let mut ready = std::mem::take(&mut asks.ready);
    let mut still = Vec::new();
    for mut held in std::mem::take(&mut asks.held) {
        held.frames += 1;
        match held.answer.take() {
            Some((true, _)) => ready.push((held.player, held.number, held.ask)),
            Some((false, reason)) => {
                let kind = held.ask.info().kind;
                refuse(asks, held.player, held.number, kind, reason, tick);
            }
            None if held.frames > SCRIPT_WAIT || !asks.scripts_check => {
                let kind = held.ask.info().kind;
                refuse(asks, held.player, held.number, kind, "no answer from the game's check".into(), tick);
            }
            None => still.push(held),
        }
    }
    asks.held = still;
    (ready, std::mem::take(&mut asks.refused))
}

/// The next request held for the C# check that it has not seen: its id,
/// who asked, and what it comes to.
pub fn next_for_script(world: &mut World) -> Option<(u32, PlayerId, VoxelAskInfo)> {
    let asks = world.resource_mut::<Asks>()?;
    let held = asks.held.iter_mut().find(|held| !held.told)?;
    held.told = true;
    Some((held.id, held.player, held.ask.info()))
}

/// Who asked request `id`, held for the C# check, and what it comes to.
pub fn held(world: &World, id: u32) -> Option<(PlayerId, VoxelAskInfo)> {
    let held = world.resource::<Asks>()?.held.iter().find(|held| held.id == id)?;
    Some((held.player, held.ask.info()))
}

/// The C# check's answer to request `id`: whether it may be made, and why
/// not.
pub fn answer_from_script(world: &mut World, id: u32, accepted: bool, reason: &str) -> bool {
    let Some(asks) = world.resource_mut::<Asks>() else { return false };
    let Some(held) = asks.held.iter_mut().find(|held| held.id == id) else { return false };
    let reason = if reason.is_empty() { "the game's check".to_string() } else { reason.to_string() };
    held.answer = Some((accepted, reason));
    true
}

/// The requests waiting for the C# check, each with who asked, its number
/// from them, its kind and how many frames it has waited.
pub fn waiting(world: &World) -> Vec<(PlayerId, u32, VoxelAskKind, u32)> {
    world
        .resource::<Asks>()
        .map(|asks| asks.held.iter().map(|h| (h.player, h.number, h.ask.info().kind, h.frames)).collect())
        .unwrap_or_default()
}

/// The last requests refused, the oldest first.
pub fn refusals(world: &World) -> Vec<Refusal> {
    world.resource::<Asks>().map(|asks| asks.refusals.iter().cloned().collect()).unwrap_or_default()
}

/// Notes the server's tick on refusals made since the last frame.
pub(crate) fn stamp_refusals(world: &mut World) {
    let tick = world.resource::<net::NetSession>().map_or(0, |s| s.tick());
    if let Some(asks) = world.resource_mut::<Asks>() {
        for refusal in asks.refusals.iter_mut().filter(|r| r.tick == 0) {
            refusal.tick = tick;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_take_the_bits_csharp_gives_them_and_requests_read_back_with_their_numbers() {
        // VoxelRequestKinds in Voxear.Networking: Dig 1, Place 2, ... CutRope 512.
        let bits: Vec<u32> = VoxelAskKind::ALL.iter().map(|kind| kind.bit()).collect();
        assert_eq!(bits, (0..10).map(|n| 1 << n).collect::<Vec<u32>>());
        let ask = VoxelAsk::Hang {
            tied: Vec3::new(1.0, 8.0, 2.0),
            holds: Vec3::new(1.0, 4.0, 2.0),
            length: 0.0,
            strength: 9000.0,
        };
        let mut bytes = vec![0x81, 0x01];
        bytes.extend_from_slice(&ask.to_bytes());
        assert_eq!(crate::voxel_sync::read_ask(&bytes), Some((129, ask.clone())));
        let info = ask.info();
        assert_eq!((info.kind, info.size), (VoxelAskKind::Hang, 4.0));
        let cut = VoxelAsk::CutRope { rope: 7, at: Vec3::ONE };
        assert_eq!(VoxelAsk::from_bytes(&cut.to_bytes()), Some(cut));
    }

    #[test]
    fn the_rules_refuse_what_is_not_allowed_too_large_or_too_many() {
        let world = World::new();
        let rules =
            VoxelAskRules { allowed: VoxelAskKind::Dig.bit(), largest: 2.0, most_points: 3, ..Default::default() };
        let dig = |n: usize| VoxelAsk::Set { points: vec![Vec3::ZERO; n], to: None };
        assert_eq!(broken(&world, &rules, PlayerId(1), &dig(3)), None);
        assert_eq!(broken(&world, &rules, PlayerId(1), &dig(4)).as_deref(), Some("more than 3 points"));
        let blast = VoxelAsk::Blast { at: Vec3::ZERO, radius: 1.0, pieces: Default::default(), throw: 0.0 };
        assert_eq!(broken(&world, &rules, PlayerId(1), &blast).as_deref(), Some("players may not blast"));
        let rules = VoxelAskRules { largest: 2.0, ..Default::default() };
        let ball = VoxelAsk::Shape(crate::voxel_volumes::VoxelEditData::ball([0.0; 3], 3.0, None));
        assert_eq!(broken(&world, &rules, PlayerId(1), &ball).as_deref(), Some("larger than 2 m"));
    }
}
