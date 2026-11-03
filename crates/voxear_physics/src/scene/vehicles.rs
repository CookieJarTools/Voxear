//! Vehicles on Objs: an Obj with a Vehicle beside its Rigidbody, driven on
//! the Wheels of its child Objs, as Jolt's vehicle constraint. Each wheel's
//! suspension is fixed above where its Obj is at rest, so the wheel hangs
//! there with half its travel either way. A vehicle is made again when its
//! settings, its wheels or its body change; before each step it takes what
//! its Vehicle asks (throttle, steering and the brakes), and after it, its
//! wheels' Objs are moved where physics put the wheels, and its speed, rpm
//! and gear written back for scripts.

use glam::{Mat4, Vec3};
use voxear_ecs::{Entity, World};
use voxear_scene::{Destroying, Parent, Transform};

use super::{PhysicsScene, Vehicle, VehicleKind, Wheel};
use crate::{BodyId, VehicleDesc, VehicleId, VehicleType, WheelDesc};

/// How far above the top of its travel a wheel's suspension is fixed, in
/// metres, so that it never reaches its fixing.
const CLEARANCE: f32 = 0.05;

/// A vehicle made, and what it was made from.
pub(super) struct Ride {
    id: VehicleId,
    key: RideKey,
}

#[derive(Clone, Debug, PartialEq)]
struct RideKey {
    vehicle: Vehicle,
    body: BodyId,
    /// Each wheel's Obj and its settings, in the order the wheels were made.
    wheels: Vec<(Entity, Wheel)>,
}

impl PhysicsScene {
    /// Makes each Vehicle's vehicle on its Obj's body and its children's
    /// Wheels, and again when any of them changed, and drives it as its
    /// Vehicle asks.
    pub(super) fn sync_vehicles(&mut self, world: &World) {
        let wanted: Vec<(Entity, Vehicle)> = world
            .query_ref::<(Entity, &Vehicle)>()
            .without::<Destroying>()
            .into_iter()
            .map(|(entity, vehicle)| (entity, *vehicle))
            .collect();
        let gone: Vec<Entity> =
            self.vehicles.keys().copied().filter(|entity| !wanted.iter().any(|(e, _)| e == entity)).collect();
        for entity in gone {
            if let Some(ride) = self.vehicles.remove(&entity) {
                self.world.remove_vehicle(ride.id);
            }
        }
        let wheels: Vec<(Entity, Entity, Wheel)> = world
            .query_ref::<(Entity, &Wheel, &Parent)>()
            .without::<Destroying>()
            .into_iter()
            .map(|(entity, wheel, parent)| (entity, parent.0, *wheel))
            .collect();
        self.wheels_at_rest.retain(|entity, _| wheels.iter().any(|(e, ..)| e == entity));
        for (entity, _, _) in &wheels {
            if !self.wheels_at_rest.contains_key(entity) {
                let at = world.get::<Transform>(*entity).map_or(Vec3::ZERO, |transform| transform.position);
                self.wheels_at_rest.insert(*entity, at);
            }
        }
        for (entity, vehicle) in wanted {
            let mine: Vec<(Entity, Wheel)> = wheels
                .iter()
                .filter(|(_, parent, _)| *parent == entity)
                .map(|(wheel, _, settings)| (*wheel, *settings))
                .collect();
            self.sync_vehicle(entity, vehicle, mine);
        }
    }

    fn sync_vehicle(&mut self, entity: Entity, vehicle: Vehicle, wheels: Vec<(Entity, Wheel)>) {
        let dynamic = self.bodies.get(&entity).filter(|kept| kept.key.rigidbody.is_some_and(|body| !body.kinematic));
        let Some(body) = dynamic.map(|kept| kept.body).filter(|_| !wheels.is_empty()) else {
            if let Some(ride) = self.vehicles.remove(&entity) {
                self.world.remove_vehicle(ride.id);
            }
            return;
        };
        let key = RideKey { vehicle: vehicle.settings(), body, wheels };
        if self.vehicles.get(&entity).is_none_or(|ride| ride.key != key) {
            if let Some(ride) = self.vehicles.remove(&entity) {
                self.world.remove_vehicle(ride.id);
            }
            let Some(id) = self.world.add_vehicle(body, &self.vehicle_desc(&key)) else {
                return tracing::warn!("a Vehicle could not be made; it drives nowhere");
            };
            self.vehicles.insert(entity, Ride { id, key });
        }
        let Some(ride) = self.vehicles.get(&entity) else { return };
        self.world.drive_vehicle(ride.id, vehicle.throttle, vehicle.steering, vehicle.brake, vehicle.handbrake);
    }

    fn vehicle_desc(&self, key: &RideKey) -> VehicleDesc {
        let wheels = key.wheels.iter().map(|(entity, wheel)| {
            let rest = self.wheels_at_rest.get(entity).copied().unwrap_or_default();
            let travel = wheel.suspension.max(0.01);
            WheelDesc {
                // Fixed above the wheel's middle at rest, half its travel up.
                position: rest + Vec3::Y * (travel * 0.5 + CLEARANCE),
                radius: wheel.radius.max(0.01),
                width: wheel.width.max(0.01),
                suspension: (CLEARANCE, travel + CLEARANCE),
                frequency: wheel.spring.max(0.01),
                damping: wheel.damping.max(0.0),
                max_steer: wheel.steer_angle.to_radians(),
                brake: wheel.brake.max(0.0),
                handbrake: wheel.handbrake.max(0.0),
                powered: wheel.powered,
            }
        });
        let kind = match key.vehicle.kind {
            VehicleKind::Car => VehicleType::Car,
            VehicleKind::Tank => VehicleType::Tank,
            VehicleKind::Motorbike => VehicleType::Motorbike,
        };
        VehicleDesc {
            kind,
            max_torque: key.vehicle.max_torque.max(0.0),
            rpm: (key.vehicle.min_rpm, key.vehicle.max_rpm),
            automatic: key.vehicle.automatic,
            max_pitch_roll: std::f32::consts::PI,
            wheels: wheels.collect(),
        }
    }

    /// Moves each vehicle's wheels' Objs where physics put the wheels, in
    /// their parent's space, and writes its speed, rpm and gear back.
    pub(super) fn write_back_vehicles(&mut self, world: &mut World) {
        for (entity, ride) in &self.vehicles {
            let state = self.world.vehicle_state(ride.id);
            if let Some(vehicle) = world.get_mut::<Vehicle>(*entity) {
                (vehicle.speed, vehicle.rpm, vehicle.gear) = (state.speed, state.rpm, state.gear);
            }
            let parent = voxear_scene::world_matrix(world, *entity).inverse();
            for (at, (wheel, _)) in ride.key.wheels.iter().enumerate() {
                let (position, rotation) = self.world.vehicle_wheel(ride.id, at);
                let local = parent * Mat4::from_rotation_translation(rotation, position);
                let (_, rotation, position) = local.to_scale_rotation_translation();
                if let Some(transform) = world.get_mut::<Transform>(*wheel) {
                    (transform.position, transform.rotation) = (position, rotation.normalize());
                }
            }
        }
    }

    /// Takes away the vehicles of a body about to go, to be made again once
    /// it is back.
    pub(super) fn remove_vehicles_of(&mut self, body: BodyId) {
        let riding: Vec<Entity> =
            self.vehicles.iter().filter(|(_, ride)| ride.key.body == body).map(|(entity, _)| *entity).collect();
        for entity in riding {
            if let Some(ride) = self.vehicles.remove(&entity) {
                self.world.remove_vehicle(ride.id);
            }
        }
    }
}
