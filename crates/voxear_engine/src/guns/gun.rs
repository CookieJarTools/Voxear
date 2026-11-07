//! A gun as data: what kind it is, how heavy and long, how hard it kicks,
//! and its sockets, the places on it hands and eyes go: the grip, where
//! the other hand holds it, the sights and the muzzle. Each is in the gun
//! model's own space, its barrel along +Z and its top +Y, from the grip.
//! A kind's preset fills in a gun's numbers; what is set over it stays.

use serde::{Deserialize, Serialize};
use voxear_core::math::{EulerRot, Mat4, Quat, Vec3};
use voxear_ecs::Component;

/// How a gun works, which decides how it is held, kicks and is reloaded.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GunKind {
    /// A turn-bolt rifle loaded from stripper clips.
    #[default]
    BoltActionRifle,
    /// A self-loading rifle fed by an en-bloc clip.
    SemiAutoRifle,
    /// A sub-machine gun that fires from an open bolt, from a box magazine.
    OpenBoltSmg,
    /// A pistol with a slide, from a box magazine.
    SlidePistol,
    /// A pistol whose breech opens by a toggle.
    TogglePistol,
    /// A machine gun fed by a belt under a top cover.
    BeltFedMg,
    /// A shotgun loaded a shell at a time into a tube, worked by a pump.
    PumpShotgun,
    /// A revolver whose barrel tips down to load it.
    TopBreakRevolver,
}

impl GunKind {
    pub const ALL: [GunKind; 8] = [
        GunKind::BoltActionRifle,
        GunKind::SemiAutoRifle,
        GunKind::OpenBoltSmg,
        GunKind::SlidePistol,
        GunKind::TogglePistol,
        GunKind::BeltFedMg,
        GunKind::PumpShotgun,
        GunKind::TopBreakRevolver,
    ];

    pub fn title(self) -> &'static str {
        match self {
            GunKind::BoltActionRifle => "Bolt-Action Rifle",
            GunKind::SemiAutoRifle => "Semi-Auto Rifle",
            GunKind::OpenBoltSmg => "Open-Bolt SMG",
            GunKind::SlidePistol => "Slide Pistol",
            GunKind::TogglePistol => "Toggle Pistol",
            GunKind::BeltFedMg => "Belt-Fed MG",
            GunKind::PumpShotgun => "Pump Shotgun",
            GunKind::TopBreakRevolver => "Top-Break Revolver",
        }
    }

    /// Whether it is held in one hand's grip, with the other hand round it.
    pub fn is_handgun(self) -> bool {
        matches!(self, GunKind::SlidePistol | GunKind::TogglePistol | GunKind::TopBreakRevolver)
    }
}

/// A place and turn on a gun, in its model's space: metres, and degrees
/// about X, then Y, then Z.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Socket {
    pub position: [f32; 3],
    pub rotation: [f32; 3],
}

impl Socket {
    pub const fn at(position: [f32; 3]) -> Self {
        Self { position, rotation: [0.0; 3] }
    }

    pub fn matrix(&self) -> Mat4 {
        Mat4::from_rotation_translation(self.turn(), Vec3::from(self.position))
    }

    pub fn turn(&self) -> Quat {
        let [x, y, z] = self.rotation.map(f32::to_radians);
        Quat::from_euler(EulerRot::XYZ, x, y, z)
    }

    /// The way a hand's fingers point and its palm faces here, in the gun's
    /// space: as a hand of `role` holds it, turned by the socket's turn.
    pub fn hand(&self, role: HandRole) -> (Vec3, Vec3) {
        let (fingers, palm) = match role {
            // The shooting hand round the grip: fingers forward and down, before they curl, palm to the left.
            HandRole::Grip => (Vec3::new(-0.15, -0.55, 0.82), Vec3::NEG_X),
            // The other hand under the fore-end, from the left: fingers across to the right, palm up.
            HandRole::Support => (Vec3::new(0.7, 0.15, 0.7), Vec3::Y),
            // The other hand round the shooting hand on a handgun: palm to the right.
            HandRole::HandgunSupport => (Vec3::new(0.15, -0.55, 0.82), Vec3::X),
        };
        let turn = self.turn();
        (turn * fingers.normalize(), turn * palm)
    }
}

/// How a hand holds a socket.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HandRole {
    Grip,
    Support,
    HandgunSupport,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Gun {
    pub kind: GunKind,
    /// How heavy it is, in kilograms, and how long, in metres: what its
    /// sway, inertia and wall check come from.
    pub mass: f32,
    pub length: f32,
    /// Rounds a minute it can fire: as fast as the trigger, or for a
    /// bolt-action, as the bolt can be worked.
    pub fire_rate: f32,
    /// How hard each shot kicks, 1 for a full-power rifle round.
    pub recoil: f32,
    /// Where the shooting hand holds it, and the other hand.
    pub grip: Socket,
    pub support: Socket,
    /// The sights: the rear and the front, and how far the eye is from the
    /// rear when aiming, in metres.
    pub rear_sight: [f32; 3],
    pub front_sight: [f32; 3],
    pub eye_relief: f32,
    /// How much the view narrows when aiming, as a field of view in
    /// degrees; 0 leaves it.
    pub aim_fov: f32,
    pub muzzle: [f32; 3],
    /// Where rounds go in: the magazine well, the clip guide or the top of
    /// the receiver, and the ejection port, where spent cases leave and a
    /// look shows the chamber.
    pub feed: [f32; 3],
    pub port: [f32; 3],
}

impl Component for Gun {}

impl Default for Gun {
    fn default() -> Self {
        Self::preset(GunKind::default())
    }
}

impl Gun {
    /// A gun of a kind, with numbers for a typical one: a Kar98k, a Garand,
    /// an MP40, a 1911, a Luger, an MG42, a trench gun and a Webley.
    pub fn preset(kind: GunKind) -> Self {
        let rifle = |mass, length, fire_rate, recoil, support: f32, rear: f32, relief| Self {
            kind,
            mass,
            length,
            fire_rate,
            recoil,
            grip: Socket::at([0.03, -0.03, -0.07]),
            support: Socket::at([-0.035, -0.045, support]),
            rear_sight: [0.0, 0.07, rear],
            front_sight: [0.0, 0.075, length - 0.15],
            eye_relief: relief,
            aim_fov: 55.0,
            muzzle: [0.0, 0.04, length - 0.12],
            feed: [0.0, 0.06, 0.12],
            port: [0.03, 0.05, 0.12],
        };
        let handgun = |mass, length: f32, fire_rate, recoil| Self {
            kind,
            mass,
            length,
            fire_rate,
            recoil,
            grip: Socket::at([0.03, -0.035, -0.06]),
            support: Socket::at([-0.035, -0.06, -0.04]),
            rear_sight: [0.0, 0.055, 0.0],
            front_sight: [0.0, 0.055, length - 0.03],
            eye_relief: 0.45,
            aim_fov: 60.0,
            muzzle: [0.0, 0.035, length - 0.01],
            feed: [0.0, -0.09, -0.03],
            port: [0.015, 0.045, 0.07],
        };
        match kind {
            GunKind::BoltActionRifle => rifle(3.9, 1.11, 15.0, 1.0, 0.42, 0.2, 0.24),
            GunKind::SemiAutoRifle => rifle(4.3, 1.1, 40.0, 1.0, 0.4, -0.03, 0.08),
            GunKind::OpenBoltSmg => Gun {
                support: Socket::at([-0.03, -0.08, 0.16]),
                feed: [0.0, -0.12, 0.12],
                ..rifle(4.0, 0.83, 550.0, 0.35, 0.2, 0.0, 0.12)
            },
            GunKind::SlidePistol => handgun(1.1, 0.22, 120.0, 0.45),
            GunKind::TogglePistol => handgun(0.87, 0.22, 120.0, 0.35),
            GunKind::BeltFedMg => rifle(11.6, 1.22, 1200.0, 0.9, 0.35, 0.05, 0.15),
            GunKind::PumpShotgun => rifle(3.6, 1.0, 40.0, 1.3, 0.38, 0.1, 0.35),
            GunKind::TopBreakRevolver => handgun(1.1, 0.28, 60.0, 0.6),
        }
    }

    /// The sights as a place and turn on the gun: at the rear sight, facing
    /// along the line to the front one, the gun's top up.
    pub fn sight_frame(&self) -> Mat4 {
        let (rear, front) = (Vec3::from(self.rear_sight), Vec3::from(self.front_sight));
        let forward = (front - rear).try_normalize().unwrap_or(Vec3::Z);
        let right = Vec3::Y.cross(forward).try_normalize().unwrap_or(Vec3::X);
        let up = forward.cross(right);
        Mat4::from_cols(right.extend(0.0), up.extend(0.0), forward.extend(0.0), rear.extend(1.0))
    }

    /// How heavy it feels, from 0 for the lightest pistol to 1 for a
    /// machine gun, flattening out so the heaviest sway no more than is
    /// believable.
    pub fn heft(&self) -> f32 {
        let load = (self.mass * (0.6 + 0.4 * self.length.min(1.4))).max(0.0);
        1.0 - (-load / 4.5).exp()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sight_frame_looks_from_the_rear_sight_to_the_front_and_heavier_guns_feel_heavier() {
        let gun = Gun::preset(GunKind::BoltActionRifle);
        let frame = gun.sight_frame();
        let along = frame.transform_vector3(Vec3::Z);
        let to_front = (Vec3::from(gun.front_sight) - Vec3::from(gun.rear_sight)).normalize();
        assert!(along.distance(to_front) < 1e-5);
        assert!(frame.transform_vector3(Vec3::Y).y > 0.99, "the top stays up");
        assert!(frame.transform_point3(Vec3::ZERO).distance(Vec3::from(gun.rear_sight)) < 1e-6);
        let hefts: Vec<f32> = [GunKind::TogglePistol, GunKind::OpenBoltSmg, GunKind::BeltFedMg]
            .map(|kind| Gun::preset(kind).heft())
            .to_vec();
        assert!(hefts[0] < hefts[1] && hefts[1] < hefts[2] && hefts[2] < 1.0, "{hefts:?}");
        let text = toml::to_string(&gun).unwrap();
        assert_eq!(toml::from_str::<Gun>(&text).unwrap(), gun);
    }
}
