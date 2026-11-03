//! Vehicles and their wheels for C#'s `Vehicle` and `Wheel`: their settings
//! read and set, what drives a vehicle set each frame, and how it goes
//! (its speed, rpm and gear) read, by codes from 200 on `physics_value`.

use voxear_ecs::{Entity, World};
use voxear_physics::scene::{Vehicle, VehicleKind, Wheel};

use super::{change, flag, number};

pub(super) const VEHICLE_HAS: i32 = 200;
const VEHICLE_ADD: i32 = 201;
const VEHICLE_REMOVE: i32 = 202;
const VEHICLE_KIND: i32 = 203;
const VEHICLE_MAX_TORQUE: i32 = 204;
const VEHICLE_MIN_RPM: i32 = 205;
const VEHICLE_MAX_RPM: i32 = 206;
const VEHICLE_AUTOMATIC: i32 = 207;
const VEHICLE_THROTTLE: i32 = 208;
const VEHICLE_STEERING: i32 = 209;
const VEHICLE_BRAKE: i32 = 210;
const VEHICLE_HANDBRAKE: i32 = 211;
const VEHICLE_SPEED: i32 = 212;
const VEHICLE_RPM: i32 = 213;
const VEHICLE_GEAR: i32 = 214;
const VEHICLE_GEARS: i32 = 215;
const VEHICLE_FIRST_GEAR: i32 = 216;
const VEHICLE_TOP_GEAR: i32 = 217;
const VEHICLE_ANTI_ROLL: i32 = 218;
const WHEEL_HAS: i32 = 220;
const WHEEL_ADD: i32 = 221;
const WHEEL_REMOVE: i32 = 222;
const WHEEL_RADIUS: i32 = 223;
const WHEEL_WIDTH: i32 = 224;
const WHEEL_SUSPENSION: i32 = 225;
const WHEEL_SPRING: i32 = 226;
const WHEEL_DAMPING: i32 = 227;
const WHEEL_STEER_ANGLE: i32 = 228;
const WHEEL_POWERED: i32 = 229;
const WHEEL_BRAKE: i32 = 230;
const WHEEL_HANDBRAKE: i32 = 231;
const WHEEL_GRIP: i32 = 232;
const WHEEL_SIDE_GRIP: i32 = 233;

pub(super) fn read(world: &World, entity: Entity, what: i32) -> Option<[f32; 4]> {
    if what >= WHEEL_HAS {
        return read_wheel(world, entity, what);
    }
    let vehicle = || world.get::<Vehicle>(entity);
    Some(match what {
        VEHICLE_HAS => flag(vehicle().is_some()),
        VEHICLE_KIND => {
            let kind = vehicle()?.kind;
            number(VehicleKind::ALL.iter().position(|k| *k == kind).unwrap_or_default() as f32)
        }
        VEHICLE_MAX_TORQUE => number(vehicle()?.max_torque),
        VEHICLE_MIN_RPM => number(vehicle()?.min_rpm),
        VEHICLE_MAX_RPM => number(vehicle()?.max_rpm),
        VEHICLE_AUTOMATIC => flag(vehicle()?.automatic),
        VEHICLE_THROTTLE => number(vehicle()?.throttle),
        VEHICLE_STEERING => number(vehicle()?.steering),
        VEHICLE_BRAKE => number(vehicle()?.brake),
        VEHICLE_HANDBRAKE => number(vehicle()?.handbrake),
        VEHICLE_SPEED => number(vehicle()?.speed),
        VEHICLE_RPM => number(vehicle()?.rpm),
        VEHICLE_GEAR => number(vehicle()?.gear as f32),
        VEHICLE_GEARS => number(vehicle()?.gears as f32),
        VEHICLE_FIRST_GEAR => number(vehicle()?.first_gear),
        VEHICLE_TOP_GEAR => number(vehicle()?.top_gear),
        VEHICLE_ANTI_ROLL => number(vehicle()?.anti_roll),
        _ => return None,
    })
}

fn read_wheel(world: &World, entity: Entity, what: i32) -> Option<[f32; 4]> {
    let wheel = || world.get::<Wheel>(entity);
    Some(match what {
        WHEEL_HAS => flag(wheel().is_some()),
        WHEEL_RADIUS => number(wheel()?.radius),
        WHEEL_WIDTH => number(wheel()?.width),
        WHEEL_SUSPENSION => number(wheel()?.suspension),
        WHEEL_SPRING => number(wheel()?.spring),
        WHEEL_DAMPING => number(wheel()?.damping),
        WHEEL_STEER_ANGLE => number(wheel()?.steer_angle),
        WHEEL_POWERED => flag(wheel()?.powered),
        WHEEL_BRAKE => number(wheel()?.brake),
        WHEEL_HANDBRAKE => number(wheel()?.handbrake),
        WHEEL_GRIP => number(wheel()?.grip),
        WHEEL_SIDE_GRIP => number(wheel()?.side_grip),
        _ => return None,
    })
}

pub(super) fn write(world: &mut World, entity: Entity, what: i32, value: [f32; 4]) -> bool {
    let (n, on) = (value[0], value[0] != 0.0);
    match what {
        VEHICLE_ADD => {
            if !world.has::<Vehicle>(entity) {
                let kind = VehicleKind::ALL.get(n as usize).copied().unwrap_or_default();
                world.insert(entity, Vehicle { kind, ..Vehicle::default() });
            }
            true
        }
        VEHICLE_REMOVE => world.remove::<Vehicle>(entity).is_some(),
        VEHICLE_KIND => change(world, entity, |v: &mut Vehicle| {
            v.kind = VehicleKind::ALL.get(n as usize).copied().unwrap_or_default();
        }),
        VEHICLE_MAX_TORQUE => change(world, entity, |v: &mut Vehicle| v.max_torque = n.max(0.0)),
        VEHICLE_MIN_RPM => change(world, entity, |v: &mut Vehicle| v.min_rpm = n.clamp(0.0, v.max_rpm)),
        VEHICLE_MAX_RPM => change(world, entity, |v: &mut Vehicle| v.max_rpm = n.max(v.min_rpm)),
        VEHICLE_AUTOMATIC => change(world, entity, |v: &mut Vehicle| v.automatic = on),
        VEHICLE_THROTTLE => change(world, entity, |v: &mut Vehicle| v.throttle = n.clamp(-1.0, 1.0)),
        VEHICLE_STEERING => change(world, entity, |v: &mut Vehicle| v.steering = n.clamp(-1.0, 1.0)),
        VEHICLE_BRAKE => change(world, entity, |v: &mut Vehicle| v.brake = n.clamp(0.0, 1.0)),
        VEHICLE_HANDBRAKE => change(world, entity, |v: &mut Vehicle| v.handbrake = n.clamp(0.0, 1.0)),
        VEHICLE_GEARS => change(world, entity, |v: &mut Vehicle| v.gears = n.round().clamp(1.0, 10.0) as u32),
        VEHICLE_FIRST_GEAR => change(world, entity, |v: &mut Vehicle| v.first_gear = n.max(0.1)),
        VEHICLE_TOP_GEAR => change(world, entity, |v: &mut Vehicle| v.top_gear = n.max(0.1)),
        VEHICLE_ANTI_ROLL => change(world, entity, |v: &mut Vehicle| v.anti_roll = n.max(0.0)),
        WHEEL_ADD => {
            if !world.has::<Wheel>(entity) {
                world.insert(entity, Wheel::default());
            }
            true
        }
        WHEEL_REMOVE => world.remove::<Wheel>(entity).is_some(),
        WHEEL_RADIUS => change(world, entity, |w: &mut Wheel| w.radius = n.max(0.01)),
        WHEEL_WIDTH => change(world, entity, |w: &mut Wheel| w.width = n.max(0.01)),
        WHEEL_SUSPENSION => change(world, entity, |w: &mut Wheel| w.suspension = n.max(0.01)),
        WHEEL_SPRING => change(world, entity, |w: &mut Wheel| w.spring = n.max(0.01)),
        WHEEL_DAMPING => change(world, entity, |w: &mut Wheel| w.damping = n.max(0.0)),
        WHEEL_STEER_ANGLE => change(world, entity, |w: &mut Wheel| w.steer_angle = n.clamp(0.0, 90.0)),
        WHEEL_POWERED => change(world, entity, |w: &mut Wheel| w.powered = on),
        WHEEL_BRAKE => change(world, entity, |w: &mut Wheel| w.brake = n.max(0.0)),
        WHEEL_HANDBRAKE => change(world, entity, |w: &mut Wheel| w.handbrake = n.max(0.0)),
        WHEEL_GRIP => change(world, entity, |w: &mut Wheel| w.grip = n.max(0.0)),
        WHEEL_SIDE_GRIP => change(world, entity, |w: &mut Wheel| w.side_grip = n.max(0.0)),
        _ => false,
    }
}
