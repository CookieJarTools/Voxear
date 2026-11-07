//! A gun's mechanism: its moving parts and what is in it. A part is a
//! piece of the gun's model, a child Obj with a Gun Part on it saying what
//! it is and how it moves: sliding along an axis, turning about one, or a
//! turn then a slide, as a turn-bolt lifts and pulls. Each part's place
//! is a number from 0, at rest, to 1, all the way. The gun's state is the
//! round in its chamber, the rounds in its feed, and what its holder
//! knows of them. Firing cycles the action as the gun's kind does.

use serde::{Deserialize, Serialize};
use voxear_core::math::{Mat4, Quat, Vec3};
use voxear_ecs::{Component, Entity, World};
use voxear_scene::{Parent, Transform};

use super::gun::{Gun, GunKind};

/// What a part of a gun is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PartRole {
    /// The bolt, worked by its handle: a turn-bolt, an open bolt's cocking
    /// handle, or a self-loader's operating rod.
    #[default]
    Bolt,
    /// A pistol's slide.
    Slide,
    /// A pistol's toggle, which folds up to open the breech.
    Toggle,
    /// A shotgun's fore-end, pumped back and forward.
    Pump,
    /// The magazine, which comes out and goes in.
    Magazine,
    /// A stripper or en-bloc clip.
    Clip,
    /// A machine gun's top cover, opened to lay in a belt.
    Cover,
    /// A top-break revolver's barrel and cylinder, tipped down to load.
    Barrel,
}

/// How a part moves from rest, in the gun's space.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum PartMotion {
    /// Slides along `axis` by `travel` metres.
    Slide { axis: [f32; 3], travel: f32 },
    /// Turns about `axis`, through `pivot`, by `angle` degrees.
    Turn { axis: [f32; 3], pivot: [f32; 3], angle: f32 },
}

impl Default for PartMotion {
    fn default() -> Self {
        PartMotion::Slide { axis: [0.0, 0.0, -1.0], travel: 0.08 }
    }
}

impl PartMotion {
    /// The move from rest at `t`, 0 to 1, in the gun's space.
    fn at(&self, t: f32) -> Mat4 {
        match *self {
            PartMotion::Slide { axis, travel } => {
                Mat4::from_translation(Vec3::from(axis).normalize_or_zero() * travel * t)
            }
            PartMotion::Turn { axis, pivot, angle } => {
                let pivot = Vec3::from(pivot);
                let turn = Quat::from_axis_angle(Vec3::from(axis).normalize_or(Vec3::Z), (angle * t).to_radians());
                Mat4::from_translation(pivot) * Mat4::from_quat(turn) * Mat4::from_translation(-pivot)
            }
        }
    }
}

/// A moving part of a gun, on the child Obj that is the part.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct GunPart {
    pub role: PartRole,
    pub motion: PartMotion,
    /// A second move after the first, as a turn-bolt's pull after its lift.
    pub then: Option<PartMotion>,
    /// Where a hand takes hold of it, in the gun's space at rest.
    pub handle: [f32; 3],
}

impl Component for GunPart {}

impl GunPart {
    /// A part of a role, moving as most guns' such parts do.
    pub fn of(role: PartRole, handle: [f32; 3]) -> Self {
        let slide = |travel| PartMotion::Slide { axis: [0.0, 0.0, -1.0], travel };
        let (motion, then) = match role {
            PartRole::Bolt => (slide(0.1), None),
            PartRole::Slide => (slide(0.06), None),
            PartRole::Toggle => (PartMotion::Turn { axis: [1.0, 0.0, 0.0], pivot: handle, angle: -70.0 }, None),
            PartRole::Pump => (slide(0.09), None),
            PartRole::Magazine | PartRole::Clip => (PartMotion::Slide { axis: [0.0, -1.0, 0.0], travel: 0.25 }, None),
            PartRole::Cover => {
                (PartMotion::Turn { axis: [1.0, 0.0, 0.0], pivot: [0.0, 0.05, 0.0], angle: -70.0 }, None)
            }
            PartRole::Barrel => {
                (PartMotion::Turn { axis: [1.0, 0.0, 0.0], pivot: [0.0, -0.02, 0.05], angle: 55.0 }, None)
            }
        };
        Self { role, motion, then, handle }
    }

    /// A turn-bolt: lifted about the bore, then pulled back.
    pub fn turn_bolt(handle: [f32; 3]) -> Self {
        Self {
            role: PartRole::Bolt,
            motion: PartMotion::Turn { axis: [0.0, 0.0, 1.0], pivot: [0.0, handle[1], 0.0], angle: 80.0 },
            then: Some(PartMotion::Slide { axis: [0.0, 0.0, -1.0], travel: 0.1 }),
            handle,
        }
    }

    /// The move from rest when the part stands at `t`: 0 to 1 through its
    /// first move, 1 to 2 through its second.
    pub fn moved(&self, t: f32) -> Mat4 {
        match self.then {
            Some(then) if t > 1.0 => then.at((t - 1.0).min(1.0)) * self.motion.at(1.0),
            _ => self.motion.at(t.clamp(0.0, 1.0)),
        }
    }

    /// How far its place runs: 2 for a part with a second move.
    pub fn full(&self) -> f32 {
        if self.then.is_some() { 2.0 } else { 1.0 }
    }
}

/// What a round in a chamber is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Chamber {
    #[default]
    Empty,
    Live,
    Spent,
}

/// What a holder knows of how many rounds a gun has.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Known {
    #[default]
    Unknown,
    /// About so many: a magazine looked at, or felt by its weight.
    Rough,
    Exact,
}

/// How a gun is fed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Feed {
    /// A magazine that comes out.
    Magazine,
    /// A magazine inside the gun, filled from stripper clips of so many.
    Clips(u32),
    /// An en-bloc clip that goes in with its rounds and out when empty.
    EnBloc,
    /// A tube filled a round at a time.
    Tube,
    /// A belt under a cover.
    Belt,
    /// A revolver's cylinder.
    Cylinder,
}

impl GunKind {
    pub fn feed(self) -> Feed {
        match self {
            GunKind::BoltActionRifle => Feed::Clips(5),
            GunKind::SemiAutoRifle => Feed::EnBloc,
            GunKind::OpenBoltSmg | GunKind::SlidePistol | GunKind::TogglePistol => Feed::Magazine,
            GunKind::BeltFedMg => Feed::Belt,
            GunKind::PumpShotgun => Feed::Tube,
            GunKind::TopBreakRevolver => Feed::Cylinder,
        }
    }

    /// How many rounds the feed holds.
    pub fn capacity(self) -> u32 {
        match self {
            GunKind::BoltActionRifle => 5,
            GunKind::SemiAutoRifle => 8,
            GunKind::OpenBoltSmg => 32,
            GunKind::SlidePistol => 7,
            GunKind::TogglePistol => 8,
            GunKind::BeltFedMg => 50,
            GunKind::PumpShotgun => 5,
            GunKind::TopBreakRevolver => 6,
        }
    }

    /// Whether firing works the action, rather than the shooter's hand.
    pub fn self_loading(self) -> bool {
        !matches!(self, GunKind::BoltActionRifle | GunKind::PumpShotgun)
    }

    /// Whether it fires from an open bolt: nothing waits in its chamber, and
    /// the bolt flies forward to fire.
    pub fn open_bolt(self) -> bool {
        matches!(self, GunKind::OpenBoltSmg | GunKind::BeltFedMg)
    }
}

/// A gun's state: its chamber, its feed, whether its action is held open,
/// its parts' places, and what its holder knows.
#[derive(Clone, Debug, PartialEq)]
pub struct GunState {
    pub chamber: Chamber,
    /// Rounds in the feed, the chamber's apart.
    pub rounds: u32,
    /// Whether the feed is in: a magazine seated, a belt laid, a clip in.
    pub fed: bool,
    /// The action held open: a bolt back on an empty magazine, a slide
    /// locked, an open bolt cocked.
    pub open: bool,
    /// Each part's place, by its Obj, and where its Obj stood at rest.
    pub parts: Vec<(Entity, f32, Mat4)>,
    pub known_feed: Known,
    pub known_chamber: Known,
    /// Rounds spent since the feed was last looked at, as the holder counts.
    pub counted: u32,
}

impl Component for GunState {}

impl GunState {
    /// A gun as it is first picked up: full, a round chambered, every part
    /// at rest, the holder knowing it is full.
    pub fn loaded(kind: GunKind) -> Self {
        let open = kind.open_bolt();
        Self {
            chamber: if open { Chamber::Empty } else { Chamber::Live },
            rounds: kind.capacity(),
            fed: true,
            open,
            parts: Vec::new(),
            known_feed: Known::Exact,
            known_chamber: Known::Exact,
            counted: 0,
        }
    }

    /// Whether a pull of the trigger fires.
    pub fn ready(&self, kind: GunKind) -> bool {
        match kind.open_bolt() {
            true => self.open && self.fed && self.rounds > 0,
            false => self.chamber == Chamber::Live,
        }
    }

    /// A shot: the round goes, and a self-loader works its action, bringing
    /// up the next or holding open on empty. Gives back whether the feed
    /// ran dry with this shot.
    pub fn shoot(&mut self, kind: GunKind) -> bool {
        if kind.open_bolt() {
            self.rounds -= 1;
            self.counted += 1;
            self.open = self.rounds > 0;
            return self.rounds == 0;
        }
        self.chamber = Chamber::Spent;
        if !kind.self_loading() {
            return false;
        }
        self.chamber = Chamber::Empty;
        if self.rounds > 0 && self.fed {
            self.rounds -= 1;
            self.counted += 1;
            self.chamber = Chamber::Live;
            return false;
        }
        // Empty: the action holds open, as a slide locks back or a Garand's clip pings out.
        self.open = kind != GunKind::TopBreakRevolver;
        if kind.feed() == Feed::EnBloc {
            self.fed = false;
        }
        true
    }

    /// The bolt worked by hand all the way: out goes whatever is chambered,
    /// and in comes the next round, if there is one.
    pub fn cycle(&mut self) {
        self.chamber = Chamber::Empty;
        if self.rounds > 0 && self.fed {
            self.rounds -= 1;
            self.counted += 1;
            self.chamber = Chamber::Live;
        }
        self.open = false;
    }

    /// A part's place, by its role, if the gun has one.
    pub fn place(&self, world: &World, role: PartRole) -> Option<f32> {
        self.parts.iter().find(|(e, ..)| world.get::<GunPart>(*e).is_some_and(|p| p.role == role)).map(|p| p.1)
    }
}

/// The gun's parts: the Objs under it with a Gun Part, each with where it
/// stands at rest.
pub fn find_parts(world: &World, gun: Entity) -> Vec<(Entity, f32, Mat4)> {
    let mut parts: Vec<(Entity, f32, Mat4)> = world
        .query_ref::<(Entity, &GunPart, &Transform, &Parent)>()
        .into_iter()
        .filter(|(_, _, _, parent)| parent.0 == gun)
        .map(|(entity, _, transform, _)| {
            let rest = Mat4::from_scale_rotation_translation(transform.scale, transform.rotation, transform.position);
            (entity, 0.0, rest)
        })
        .collect();
    parts.sort_by_key(|(entity, ..)| entity.index());
    parts
}

/// Moves each part's Obj to its place.
pub fn show_parts(world: &mut World, state: &GunState) {
    for &(entity, place, rest) in &state.parts {
        let Some(part) = world.get::<GunPart>(entity).cloned() else { continue };
        let (scale, rotation, position) = (part.moved(place) * rest).to_scale_rotation_translation();
        if let Some(transform) = world.get_mut::<Transform>(entity) {
            (transform.position, transform.rotation, transform.scale) = (position, rotation, scale);
        }
    }
}

/// The state of a gun, made the first time it is needed.
pub fn state_of(world: &mut World, gun: Entity) -> Option<GunState> {
    if let Some(state) = world.get::<GunState>(gun) {
        return Some(state.clone());
    }
    let kind = world.get::<Gun>(gun)?.kind;
    let mut state = GunState::loaded(kind);
    state.parts = find_parts(world, gun);
    world.insert(gun, state.clone());
    Some(state)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn self_loaders_bring_up_the_next_round_and_hold_open_when_empty() {
        let mut pistol = GunState::loaded(GunKind::SlidePistol);
        let mut shots = 0;
        while pistol.ready(GunKind::SlidePistol) {
            let dry = pistol.shoot(GunKind::SlidePistol);
            shots += 1;
            assert_eq!(dry, !pistol.ready(GunKind::SlidePistol));
        }
        assert_eq!(shots, 8, "seven in the magazine and one chambered");
        assert!(pistol.open, "the slide locks back");

        let mut garand = GunState::loaded(GunKind::SemiAutoRifle);
        while garand.ready(GunKind::SemiAutoRifle) {
            garand.shoot(GunKind::SemiAutoRifle);
        }
        assert!(!garand.fed && garand.open, "the empty clip pings out and the bolt stays back");

        let mut smg = GunState::loaded(GunKind::OpenBoltSmg);
        assert_eq!(smg.chamber, Chamber::Empty, "an open bolt keeps nothing chambered");
        let mut shots = 0;
        while smg.ready(GunKind::OpenBoltSmg) {
            smg.shoot(GunKind::OpenBoltSmg);
            shots += 1;
        }
        assert_eq!(shots, 32);
    }

    #[test]
    fn a_bolt_action_fires_once_then_needs_its_bolt_worked() {
        let kind = GunKind::BoltActionRifle;
        let mut rifle = GunState::loaded(kind);
        rifle.shoot(kind);
        assert_eq!(rifle.chamber, Chamber::Spent);
        assert!(!rifle.ready(kind));
        rifle.cycle();
        assert!(rifle.ready(kind) && rifle.rounds == 4);
    }

    #[test]
    fn a_turn_bolt_lifts_then_pulls() {
        let bolt = GunPart::turn_bolt([0.05, 0.03, 0.1]);
        let handle = Vec3::new(0.05, 0.03, 0.1);
        let lifted = bolt.moved(1.0).transform_point3(handle);
        assert!(lifted.y > handle.y + 0.03 && (lifted.z - handle.z).abs() < 1e-4, "lifted, not pulled: {lifted}");
        let pulled = bolt.moved(2.0).transform_point3(handle);
        assert!(
            (pulled.z - (handle.z - 0.1)).abs() < 1e-4 && (pulled.y - lifted.y).abs() < 1e-4,
            "pulled back: {pulled}"
        );
        assert_eq!(bolt.full(), 2.0);
    }
}
