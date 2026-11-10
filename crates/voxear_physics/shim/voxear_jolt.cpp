// Voxear's shim over Jolt Physics: a small C API that the Rust side declares
// in src/ffi.rs, so Jolt's C++ stays on this side. A world owns its shapes,
// bodies and characters, each named by a number: a shape by its slot, a body
// by Jolt's own id. Contacts are kept as they happen, on Jolt's job threads,
// and handed over after each step.

#include <Jolt/Jolt.h>

#include <Jolt/Core/Factory.h>
#include <Jolt/Core/JobSystemThreadPool.h>
#include <Jolt/Core/TempAllocator.h>
#include <Jolt/Physics/Body/BodyCreationSettings.h>
#include <Jolt/Physics/Body/BodyLock.h>
#include <Jolt/Physics/Body/BodyLockMulti.h>
#include <Jolt/Physics/Character/CharacterVirtual.h>
#include <Jolt/Physics/Collision/CastResult.h>
#include <Jolt/Physics/Collision/CollidePointResult.h>
#include <Jolt/Physics/Collision/CollideShape.h>
#include <Jolt/Physics/Collision/CollisionCollectorImpl.h>
#include <Jolt/Physics/Collision/RayCast.h>
#include <Jolt/Physics/Collision/Shape/BoxShape.h>
#include <Jolt/Physics/Collision/Shape/CapsuleShape.h>
#include <Jolt/Physics/Collision/Shape/ConvexHullShape.h>
#include <Jolt/Physics/Collision/Shape/EmptyShape.h>
#include <Jolt/Physics/Collision/Shape/HeightFieldShape.h>
#include <Jolt/Physics/Collision/Shape/MeshShape.h>
#include <Jolt/Physics/Collision/Shape/RotatedTranslatedShape.h>
#include <Jolt/Physics/Collision/Shape/ScaledShape.h>
#include <Jolt/Physics/Collision/Shape/SphereShape.h>
#include <Jolt/Physics/Collision/ShapeCast.h>
#include <Jolt/Physics/Constraints/DistanceConstraint.h>
#include <Jolt/Physics/Constraints/FixedConstraint.h>
#include <Jolt/Physics/Constraints/HingeConstraint.h>
#include <Jolt/Physics/Constraints/PointConstraint.h>
#include <Jolt/Physics/PhysicsSettings.h>
#include <Jolt/Physics/PhysicsSystem.h>
#include <Jolt/Physics/Vehicle/MotorcycleController.h>
#include <Jolt/Physics/Vehicle/TrackedVehicleController.h>
#include <Jolt/Physics/Vehicle/VehicleCollisionTester.h>
#include <Jolt/Physics/Vehicle/VehicleConstraint.h>
#include <Jolt/Physics/Vehicle/WheeledVehicleController.h>
#include <Jolt/RegisterTypes.h>

#include <algorithm>
#include <cstdint>
#include <iterator>
#include <mutex>
#include <thread>
#include <vector>

using namespace JPH;

namespace {

// Each of Voxear's 32 layers is two object layers, what does not move on it
// and what does: an object layer is the Voxear layer times two, plus one
// when it moves. What does not move never collides with itself, and two
// layers collide as the project's matrix says. The broad phase keeps only
// the still and the moving apart.
constexpr ObjectLayer STILL = 0;
constexpr ObjectLayer MOVING = 1;
constexpr uint32_t BROAD_LAYERS = 2;
constexpr uint32_t VOXEAR_LAYERS = 32;

static ObjectLayer object_layer(uint32_t layer, bool moving) {
	return ObjectLayer((layer % VOXEAR_LAYERS) * 2 + (moving ? 1 : 0));
}

class LayerPairs final : public ObjectLayerPairFilter {
public:
	LayerPairs() { std::fill(std::begin(collide), std::end(collide), 0xffffffffu); }
	bool ShouldCollide(ObjectLayer a, ObjectLayer b) const override {
		return ((a & 1) || (b & 1)) && (collide[a >> 1] >> (b >> 1) & 1u);
	}
	// Bit `b` of `collide[a]` is set when layers `a` and `b` collide.
	uint32_t collide[VOXEAR_LAYERS];
};

class BroadLayers final : public BroadPhaseLayerInterface {
public:
	uint GetNumBroadPhaseLayers() const override { return BROAD_LAYERS; }
	BroadPhaseLayer GetBroadPhaseLayer(ObjectLayer layer) const override { return BroadPhaseLayer(BroadPhaseLayer::Type(layer & 1)); }
#if defined(JPH_EXTERNAL_PROFILE) || defined(JPH_PROFILE_ENABLED)
	const char *GetBroadPhaseLayerName(BroadPhaseLayer layer) const override { return layer.GetValue() == STILL ? "Still" : "Moving"; }
#endif
};

class LayerVsBroad final : public ObjectVsBroadPhaseLayerFilter {
public:
	bool ShouldCollide(ObjectLayer layer, BroadPhaseLayer broad) const override {
		return (layer & 1) || broad.GetValue() == MOVING;
	}
};

// What a query asks after: the Voxear layers in its mask.
class LayerMaskFilter final : public ObjectLayerFilter {
public:
	explicit LayerMaskFilter(uint32_t mask) : mask(mask) {}
	bool ShouldCollide(ObjectLayer layer) const override { return mask >> (layer >> 1) & 1u; }

private:
	uint32_t mask;
};

struct Contact {
	uint32_t kind; // 0 began, 1 ended
	uint32_t a;
	uint32_t b;
	float point[3];
	float normal[3];
};

// Contacts as Jolt reports them, from its job threads.
class Contacts final : public ContactListener {
public:
	void OnContactAdded(const Body &a, const Body &b, const ContactManifold &manifold, ContactSettings &) override {
		Contact contact { 0, a.GetID().GetIndexAndSequenceNumber(), b.GetID().GetIndexAndSequenceNumber(), {}, {} };
		RVec3 point = manifold.GetWorldSpaceContactPointOn1(0);
		Vec3 normal = manifold.mWorldSpaceNormal;
		contact.point[0] = float(point.GetX());
		contact.point[1] = float(point.GetY());
		contact.point[2] = float(point.GetZ());
		contact.normal[0] = normal.GetX();
		contact.normal[1] = normal.GetY();
		contact.normal[2] = normal.GetZ();
		std::lock_guard<std::mutex> lock(mutex);
		events.push_back(contact);
	}

	void OnContactRemoved(const SubShapeIDPair &pair) override {
		Contact contact { 1, pair.GetBody1ID().GetIndexAndSequenceNumber(), pair.GetBody2ID().GetIndexAndSequenceNumber(), {}, {} };
		std::lock_guard<std::mutex> lock(mutex);
		events.push_back(contact);
	}

	std::mutex mutex;
	std::vector<Contact> events;
};

// A vehicle: its constraint, which Jolt steps as a step listener, the test
// that finds the ground under its wheels, and what kind it is.
struct Vehicle {
	Ref<VehicleConstraint> constraint;
	Ref<VehicleCollisionTester> tester;
	uint32_t kind; // 0 a car, 1 a tank, 2 a motorbike
};

struct Character {
	Ref<CharacterVirtual> inner;
	float height_offset;
	// Its Voxear layer, as it moves.
	uint32_t layer;
};

} // namespace

struct VxWorld {
	LayerPairs layer_pairs;
	BroadLayers broad_layers;
	LayerVsBroad layer_vs_broad;
	Contacts contacts;
	// Scratch memory for a step, falling back to the heap when a busy step needs more.
	TempAllocatorImplWithMallocFallback temp { 16 * 1024 * 1024 };
	JobSystemThreadPool jobs;
	PhysicsSystem system;
	std::vector<RefConst<Shape>> shapes;
	std::vector<Character> characters;
	std::vector<Ref<Constraint>> joints;
	// For each joint held to the world, the still body it is held to, which
	// moves with everything else when the world's origin does; none for one
	// between two bodies.
	std::vector<BodyID> anchors;
	std::vector<Vehicle> vehicles;

	VxWorld(uint32_t max_bodies)
		: jobs(cMaxPhysicsJobs, cMaxPhysicsBarriers, int(std::max(1u, std::thread::hardware_concurrency() - 1))) {
		// Touching pairs past these are dropped for the step, which a busy scene can afford.
		uint32_t max_contacts = std::min<uint32_t>(max_bodies, 10240);
		system.Init(max_bodies, 0, max_bodies, max_contacts, broad_layers, layer_vs_broad, layer_pairs);
		system.SetContactListener(&contacts);
	}
};

extern "C" {

struct VxBodyDesc {
	uint32_t shape;
	float position[3];
	float rotation[4];
	uint32_t motion; // 0 static, 1 kinematic, 2 dynamic
	float mass; // 0 works it out from the shape
	float friction;
	float restitution;
	float linear_damping;
	float angular_damping;
	float gravity_factor;
	uint32_t sensor;
	uint32_t continuous;
	uint64_t user_data;
	uint32_t layer; // 0 to 31, Voxear's
};

struct VxHit {
	uint32_t body;
	float fraction;
	float point[3];
	float normal[3];
};

struct VxCharacterDesc {
	float position[3];
	float height;
	float radius;
	float max_slope_degrees;
	float step_height;
	float mass;
	uint32_t layer;
};

struct VxCharacterState {
	float position[3];
	float velocity[3];
	uint32_t ground; // 0 on the ground, 1 on a steep slope, 2 against something, 3 in the air
	float ground_normal[3];
};

static std::once_flag registered;

VxWorld *vx_world_create(uint32_t max_bodies) {
	std::call_once(registered, [] {
		RegisterDefaultAllocator();
		Factory::sInstance = new Factory();
		RegisterTypes();
	});
	return new VxWorld(max_bodies);
}

void vx_world_destroy(VxWorld *world) { delete world; }

void vx_world_set_gravity(VxWorld *world, float x, float y, float z) { world->system.SetGravity(Vec3(x, y, z)); }

// Which of Voxear's layers collide: bit `b` of `collide[a]` set when `a`
// and `b` do.
void vx_world_set_layers(VxWorld *world, const uint32_t collide[32]) {
	std::copy(collide, collide + VOXEAR_LAYERS, world->layer_pairs.collide);
}

void vx_world_step(VxWorld *world, float dt, int steps) {
	world->system.Update(dt, std::max(1, steps), &world->temp, &world->jobs);
}

// Moves everything by `by`, as the world's origin moves: every body, without
// waking those asleep or changing how fast any goes, and every character,
// which then finds what it touches where it is now.
void vx_world_shift(VxWorld *world, const float by[3]) {
	Vec3 offset(by[0], by[1], by[2]);
	BodyInterface &bodies = world->system.GetBodyInterface();
	BodyIDVector ids;
	world->system.GetBodies(ids);
	for (BodyID id : ids) {
		bodies.SetPosition(id, bodies.GetPosition(id) + offset, EActivation::DontActivate);
	}
	world->system.OptimizeBroadPhase();
	for (Character &character : world->characters) {
		if (character.inner == nullptr) {
			continue;
		}
		character.inner->SetPosition(character.inner->GetPosition() + offset);
		ObjectLayer layer = object_layer(character.layer, true);
		DefaultBroadPhaseLayerFilter broad(world->layer_vs_broad, layer);
		DefaultObjectLayerFilter layers(world->layer_pairs, layer);
		character.inner->RefreshContacts(broad, layers, {}, {}, world->temp);
	}
}

// Shapes. Each is kept in a slot until released; 0 is none.

static uint32_t keep(VxWorld *world, const Shape *shape) {
	if (shape == nullptr) {
		return 0;
	}
	for (size_t at = 0; at < world->shapes.size(); ++at) {
		if (world->shapes[at] == nullptr) {
			world->shapes[at] = shape;
			return uint32_t(at + 1);
		}
	}
	world->shapes.push_back(shape);
	return uint32_t(world->shapes.size());
}

static const Shape *shape_of(VxWorld *world, uint32_t shape) {
	return shape > 0 && shape <= world->shapes.size() ? world->shapes[shape - 1].GetPtr() : nullptr;
}

uint32_t vx_shape_box(VxWorld *world, float hx, float hy, float hz) {
	float convex = std::min(0.05f, 0.5f * std::min(hx, std::min(hy, hz)));
	return keep(world, new BoxShape(Vec3(hx, hy, hz), convex));
}

uint32_t vx_shape_sphere(VxWorld *world, float radius) { return keep(world, new SphereShape(radius)); }

uint32_t vx_shape_capsule(VxWorld *world, float half_height, float radius) {
	return keep(world, new CapsuleShape(half_height, radius));
}

uint32_t vx_shape_convex_hull(VxWorld *world, const float *points, uint32_t count) {
	Array<Vec3> hull;
	hull.reserve(count);
	for (uint32_t i = 0; i < count; ++i) {
		hull.push_back(Vec3(points[i * 3], points[i * 3 + 1], points[i * 3 + 2]));
	}
	ConvexHullShapeSettings settings(hull);
	auto made = settings.Create();
	return made.IsValid() ? keep(world, made.Get().GetPtr()) : 0;
}

uint32_t vx_shape_mesh(VxWorld *world, const float *vertices, uint32_t vertex_count, const uint32_t *indices, uint32_t triangle_count) {
	VertexList points;
	points.reserve(vertex_count);
	for (uint32_t i = 0; i < vertex_count; ++i) {
		points.push_back(Float3(vertices[i * 3], vertices[i * 3 + 1], vertices[i * 3 + 2]));
	}
	IndexedTriangleList triangles;
	triangles.reserve(triangle_count);
	for (uint32_t i = 0; i < triangle_count; ++i) {
		triangles.push_back(IndexedTriangle(indices[i * 3], indices[i * 3 + 1], indices[i * 3 + 2]));
	}
	MeshShapeSettings settings(points, triangles);
	auto made = settings.Create();
	return made.IsValid() ? keep(world, made.Get().GetPtr()) : 0;
}

// Ground as heights: `count` by `count` samples, row after row along z,
// `spacing` apart, from the origin along +x and +z; FLT_MAX where there is
// no ground. Quads split from their lowest corner to their highest.
uint32_t vx_shape_heightfield(VxWorld *world, const float *samples, uint32_t count, float spacing) {
	HeightFieldShapeSettings settings(samples, Vec3::sZero(), Vec3(spacing, 1.0f, spacing), count);
	auto made = settings.Create();
	return made.IsValid() ? keep(world, made.Get().GetPtr()) : 0;
}

// A shape moved, turned and scaled from where it was made, as a collider
// placed on its Obj.
uint32_t vx_shape_placed(VxWorld *world, uint32_t shape, const float offset[3], const float rotation[4], const float scale[3]) {
	const Shape *inner = shape_of(world, shape);
	if (inner == nullptr) {
		return 0;
	}
	RefConst<Shape> placed = inner;
	Vec3 scaled(scale[0], scale[1], scale[2]);
	if (!scaled.IsClose(Vec3::sReplicate(1.0f))) {
		placed = new ScaledShape(placed, scaled);
	}
	Vec3 by(offset[0], offset[1], offset[2]);
	Quat turned(rotation[0], rotation[1], rotation[2], rotation[3]);
	if (!by.IsNearZero() || !turned.IsClose(Quat::sIdentity())) {
		placed = new RotatedTranslatedShape(by, turned, placed);
	}
	return keep(world, placed.GetPtr());
}

void vx_shape_release(VxWorld *world, uint32_t shape) {
	if (shape > 0 && shape <= world->shapes.size()) {
		world->shapes[shape - 1] = nullptr;
	}
}

// Bodies, by Jolt's id; 0xffffffff is none.

static BodyID id_of(uint32_t body) { return BodyID(body); }

uint32_t vx_body_add(VxWorld *world, const VxBodyDesc *desc) {
	const Shape *shape = shape_of(world, desc->shape);
	if (shape == nullptr) {
		return BodyID::cInvalidBodyID;
	}
	EMotionType motion = desc->motion == 0 ? EMotionType::Static : desc->motion == 1 ? EMotionType::Kinematic : EMotionType::Dynamic;
	BodyCreationSettings settings(shape, RVec3(desc->position[0], desc->position[1], desc->position[2]),
		Quat(desc->rotation[0], desc->rotation[1], desc->rotation[2], desc->rotation[3]).Normalized(), motion,
		object_layer(desc->layer, motion != EMotionType::Static));
	settings.mFriction = desc->friction;
	settings.mRestitution = desc->restitution;
	settings.mLinearDamping = desc->linear_damping;
	settings.mAngularDamping = desc->angular_damping;
	settings.mGravityFactor = desc->gravity_factor;
	settings.mIsSensor = desc->sensor != 0;
	settings.mUserData = desc->user_data;
	settings.mMotionQuality = desc->continuous != 0 ? EMotionQuality::LinearCast : EMotionQuality::Discrete;
	// A kinematic sensor still hears what it touches that does not move.
	settings.mCollideKinematicVsNonDynamic = desc->sensor != 0;
	if (desc->mass > 0.0f && motion == EMotionType::Dynamic) {
		settings.mOverrideMassProperties = EOverrideMassProperties::CalculateInertia;
		settings.mMassPropertiesOverride.mMass = desc->mass;
	}
	BodyInterface &bodies = world->system.GetBodyInterface();
	Body *body = bodies.CreateBody(settings);
	if (body == nullptr) {
		return BodyID::cInvalidBodyID;
	}
	bodies.AddBody(body->GetID(), motion == EMotionType::Static ? EActivation::DontActivate : EActivation::Activate);
	return body->GetID().GetIndexAndSequenceNumber();
}

void vx_body_remove(VxWorld *world, uint32_t body) {
	BodyInterface &bodies = world->system.GetBodyInterface();
	bodies.RemoveBody(id_of(body));
	bodies.DestroyBody(id_of(body));
}

void vx_body_get_transform(VxWorld *world, uint32_t body, float position[3], float rotation[4]) {
	RVec3 at;
	Quat turned;
	world->system.GetBodyInterface().GetPositionAndRotation(id_of(body), at, turned);
	position[0] = float(at.GetX());
	position[1] = float(at.GetY());
	position[2] = float(at.GetZ());
	rotation[0] = turned.GetX();
	rotation[1] = turned.GetY();
	rotation[2] = turned.GetZ();
	rotation[3] = turned.GetW();
}

void vx_body_set_transform(VxWorld *world, uint32_t body, const float position[3], const float rotation[4]) {
	world->system.GetBodyInterface().SetPositionAndRotation(id_of(body), RVec3(position[0], position[1], position[2]),
		Quat(rotation[0], rotation[1], rotation[2], rotation[3]).Normalized(), EActivation::Activate);
}

void vx_body_move_kinematic(VxWorld *world, uint32_t body, const float position[3], const float rotation[4], float dt) {
	world->system.GetBodyInterface().MoveKinematic(id_of(body), RVec3(position[0], position[1], position[2]),
		Quat(rotation[0], rotation[1], rotation[2], rotation[3]).Normalized(), dt);
}

void vx_body_get_velocity(VxWorld *world, uint32_t body, float linear[3], float angular[3]) {
	BodyInterface &bodies = world->system.GetBodyInterface();
	Vec3 l = bodies.GetLinearVelocity(id_of(body));
	Vec3 a = bodies.GetAngularVelocity(id_of(body));
	linear[0] = l.GetX();
	linear[1] = l.GetY();
	linear[2] = l.GetZ();
	angular[0] = a.GetX();
	angular[1] = a.GetY();
	angular[2] = a.GetZ();
}

void vx_body_set_velocity(VxWorld *world, uint32_t body, const float linear[3], const float angular[3]) {
	world->system.GetBodyInterface().SetLinearAndAngularVelocity(id_of(body), Vec3(linear[0], linear[1], linear[2]),
		Vec3(angular[0], angular[1], angular[2]));
}

// A push: 0 a force over the step, 1 an impulse at once, 2 a torque, 3 an
// angular impulse; `at` puts a force or impulse at a point in the world.
void vx_body_push(VxWorld *world, uint32_t body, uint32_t kind, const float amount[3], const float *at) {
	BodyInterface &bodies = world->system.GetBodyInterface();
	Vec3 by(amount[0], amount[1], amount[2]);
	BodyID id = id_of(body);
	switch (kind) {
	case 0:
		if (at != nullptr) {
			bodies.AddForce(id, by, RVec3(at[0], at[1], at[2]));
		} else {
			bodies.AddForce(id, by);
		}
		break;
	case 1:
		if (at != nullptr) {
			bodies.AddImpulse(id, by, RVec3(at[0], at[1], at[2]));
		} else {
			bodies.AddImpulse(id, by);
		}
		break;
	case 2:
		bodies.AddTorque(id, by);
		break;
	default:
		bodies.AddAngularImpulse(id, by);
		break;
	}
}

int vx_body_is_active(VxWorld *world, uint32_t body) { return world->system.GetBodyInterface().IsActive(id_of(body)) ? 1 : 0; }

// Wakes a body, or puts it to sleep until something moves it.
void vx_body_set_active(VxWorld *world, uint32_t body, int active) {
	if (active != 0) {
		world->system.GetBodyInterface().ActivateBody(id_of(body));
	} else {
		world->system.GetBodyInterface().DeactivateBody(id_of(body));
	}
}

uint64_t vx_body_user_data(VxWorld *world, uint32_t body) { return world->system.GetBodyInterface().GetUserData(id_of(body)); }

// Queries.

// What a query passes over: one body, and sensors unless `sensors`.
class QueryFilter : public BodyFilter {
public:
	QueryFilter(uint32_t ignore, bool sensors) : ignore(id_of(ignore)), sensors(sensors) {}
	bool ShouldCollide(const BodyID &body) const override { return body != ignore; }
	bool ShouldCollideLocked(const Body &body) const override { return sensors || !body.IsSensor(); }

private:
	BodyID ignore;
	bool sensors;
};

// The query flags: 1 counts sensors (triggers) too.
constexpr uint32_t QUERY_SENSORS = 1;

// The nearest body a ray meets. As in Unity, a ray that starts inside a
// shape passes out of it without meeting it.
int vx_raycast(VxWorld *world, const float origin[3], const float direction[3], uint32_t ignore, uint32_t flags, uint32_t mask, VxHit *hit) {
	RRayCast ray { RVec3(origin[0], origin[1], origin[2]), Vec3(direction[0], direction[1], direction[2]) };
	RayCastSettings settings;
	settings.mTreatConvexAsSolid = false;
	settings.SetBackFaceMode(EBackFaceMode::IgnoreBackFaces);
	ClosestHitCollisionCollector<CastRayCollector> found;
	QueryFilter filter(ignore, (flags & QUERY_SENSORS) != 0);
	LayerMaskFilter layers(mask);
	world->system.GetNarrowPhaseQuery().CastRay(ray, settings, found, {}, layers, filter);
	if (!found.HadHit()) {
		return 0;
	}
	const RayCastResult &result = found.mHit;
	RVec3 point = ray.GetPointOnRay(result.mFraction);
	hit->body = result.mBodyID.GetIndexAndSequenceNumber();
	hit->fraction = result.mFraction;
	hit->point[0] = float(point.GetX());
	hit->point[1] = float(point.GetY());
	hit->point[2] = float(point.GetZ());
	BodyLockRead lock(world->system.GetBodyLockInterface(), result.mBodyID);
	Vec3 normal = lock.Succeeded() ? lock.GetBody().GetWorldSpaceSurfaceNormal(result.mSubShapeID2, point) : Vec3::sAxisY();
	hit->normal[0] = normal.GetX();
	hit->normal[1] = normal.GetY();
	hit->normal[2] = normal.GetZ();
	return 1;
}

// The bodies whose shapes hold a point, up to `capacity`; returns how many.
int vx_overlap_point(VxWorld *world, const float point[3], uint32_t *bodies, int capacity) {
	AllHitCollisionCollector<CollidePointCollector> found;
	world->system.GetNarrowPhaseQuery().CollidePoint(RVec3(point[0], point[1], point[2]), found);
	int count = 0;
	for (const CollidePointResult &hit : found.mHits) {
		if (count < capacity) {
			bodies[count] = hit.mBodyID.GetIndexAndSequenceNumber();
		}
		++count;
	}
	return count;
}

// The bodies a sphere overlaps, each once, up to `capacity`; returns how many.
int vx_overlap_sphere(VxWorld *world, const float center[3], float radius, uint32_t flags, uint32_t mask, uint32_t *bodies, int capacity) {
	SphereShape sphere(std::max(radius, 1.0e-4f));
	sphere.SetEmbedded();
	CollideShapeSettings settings;
	AllHitCollisionCollector<CollideShapeCollector> found;
	QueryFilter filter(BodyID::cInvalidBodyID, (flags & QUERY_SENSORS) != 0);
	LayerMaskFilter layers(mask);
	RMat44 at = RMat44::sTranslation(RVec3(center[0], center[1], center[2]));
	world->system.GetNarrowPhaseQuery().CollideShape(&sphere, Vec3::sOne(), at, settings, RVec3::sZero(), found, {}, layers, filter);
	std::vector<BodyID> seen;
	for (const CollideShapeResult &hit : found.mHits) {
		if (std::find(seen.begin(), seen.end(), hit.mBodyID2) != seen.end()) {
			continue;
		}
		if (int(seen.size()) < capacity) {
			bodies[seen.size()] = hit.mBodyID2.GetIndexAndSequenceNumber();
		}
		seen.push_back(hit.mBodyID2);
	}
	return int(seen.size());
}

// Contacts since the last call, up to `capacity`, oldest first; returns how
// many there were, which may be more.
struct VxContact {
	uint32_t kind;
	uint32_t a;
	uint32_t b;
	float point[3];
	float normal[3];
};

int vx_contacts(VxWorld *world, VxContact *out, int capacity) {
	std::lock_guard<std::mutex> lock(world->contacts.mutex);
	auto &events = world->contacts.events;
	int count = int(events.size());
	int taken = std::min(count, capacity);
	for (int i = 0; i < taken; ++i) {
		const Contact &from = events[i];
		out[i] = VxContact { from.kind, from.a, from.b, { from.point[0], from.point[1], from.point[2] },
			{ from.normal[0], from.normal[1], from.normal[2] } };
	}
	events.erase(events.begin(), events.begin() + taken);
	return count;
}

// Characters: a capsule that walks, slides along walls, climbs steps and
// stands on slopes, moved by velocity rather than by forces.

uint32_t vx_character_add(VxWorld *world, const VxCharacterDesc *desc) {
	float half = std::max(0.01f, 0.5f * desc->height - desc->radius);
	Ref<CharacterVirtualSettings> settings = new CharacterVirtualSettings();
	// The capsule stands on the character's position, at its feet.
	settings->mShape = RotatedTranslatedShapeSettings(Vec3(0, half + desc->radius, 0), Quat::sIdentity(),
		new CapsuleShape(half, desc->radius)).Create().Get();
	// A kinematic body inside follows the character, so that triggers notice
	// it and bodies thrown at it hit it; its own moves pass over it.
	settings->mInnerBodyShape = settings->mShape;
	settings->mInnerBodyLayer = object_layer(desc->layer, true);
	settings->mMaxSlopeAngle = DegreesToRadians(desc->max_slope_degrees);
	settings->mMass = desc->mass;
	settings->mSupportingVolume = Plane(Vec3::sAxisY(), -desc->radius);
	Character character;
	character.height_offset = 0.0f;
	character.layer = desc->layer;
	character.inner = new CharacterVirtual(settings, RVec3(desc->position[0], desc->position[1], desc->position[2]),
		Quat::sIdentity(), 0, &world->system);
	world->characters.push_back(character);
	return uint32_t(world->characters.size());
}

uint32_t vx_character_body(VxWorld *world, uint32_t character) {
	if (character == 0 || character > world->characters.size() || world->characters[character - 1].inner == nullptr) {
		return BodyID::cInvalidBodyID;
	}
	return world->characters[character - 1].inner->GetInnerBodyID().GetIndexAndSequenceNumber();
}

void vx_character_remove(VxWorld *world, uint32_t character) {
	if (character > 0 && character <= world->characters.size()) {
		world->characters[character - 1].inner = nullptr;
	}
}

// Moves a character for a step at `velocity`, its own; gravity is the
// caller's to add, so a jump is a velocity up.
void vx_character_move(VxWorld *world, uint32_t character, const float velocity[3], float dt, float step_height, VxCharacterState *state) {
	if (character == 0 || character > world->characters.size() || world->characters[character - 1].inner == nullptr) {
		return;
	}
	CharacterVirtual &inner = *world->characters[character - 1].inner;
	inner.SetLinearVelocity(Vec3(velocity[0], velocity[1], velocity[2]));
	CharacterVirtual::ExtendedUpdateSettings settings;
	settings.mWalkStairsStepUp = Vec3(0, step_height, 0);
	settings.mStickToFloorStepDown = Vec3(0, -step_height, 0);
	ObjectLayer layer = object_layer(world->characters[character - 1].layer, true);
	DefaultBroadPhaseLayerFilter broad(world->layer_vs_broad, layer);
	DefaultObjectLayerFilter layers(world->layer_pairs, layer);
	inner.ExtendedUpdate(dt, world->system.GetGravity(), settings, broad, layers, {}, {}, world->temp);
	RVec3 at = inner.GetPosition();
	Vec3 moving = inner.GetLinearVelocity();
	Vec3 ground = inner.GetGroundNormal();
	state->position[0] = float(at.GetX());
	state->position[1] = float(at.GetY());
	state->position[2] = float(at.GetZ());
	state->velocity[0] = moving.GetX();
	state->velocity[1] = moving.GetY();
	state->velocity[2] = moving.GetZ();
	state->ground = uint32_t(inner.GetGroundState());
	state->ground_normal[0] = ground.GetX();
	state->ground_normal[1] = ground.GetY();
	state->ground_normal[2] = ground.GetZ();
}

void vx_character_set_position(VxWorld *world, uint32_t character, const float position[3]) {
	if (character > 0 && character <= world->characters.size() && world->characters[character - 1].inner != nullptr) {
		world->characters[character - 1].inner->SetPosition(RVec3(position[0], position[1], position[2]));
	}
}

// Joints between two bodies: 0 fixed, 1 a point both turn about, 2 a hinge
// about an axis, 3 a distance kept between two points. Returns a number for
// the joint, or 0.

// A joint between two bodies, or between `a` and the world when `b` is
// none (0xffffffff).
uint32_t vx_joint_add(VxWorld *world, uint32_t kind, uint32_t a, uint32_t b, const float point[3], const float axis[3], float min, float max) {
	RVec3 at(point[0], point[1], point[2]);
	// One held to the world is held to a still body with nothing to touch, where the joint is, or for a fixed
	// joint where its body is, so the point it holds by is found there.
	BodyID anchor;
	if (b == BodyID::cInvalidBodyID) {
		BodyInterface &bodies = world->system.GetBodyInterface();
		RVec3 place = kind == 0 ? bodies.GetCenterOfMassPosition(id_of(a)) : at;
		BodyCreationSettings still(new EmptyShape(), place, Quat::sIdentity(), EMotionType::Static, object_layer(0, false));
		anchor = bodies.CreateAndAddBody(still, EActivation::DontActivate);
		if (anchor.IsInvalid()) {
			return 0;
		}
		b = anchor.GetIndexAndSequenceNumber();
	}
	BodyID ids[2] = { id_of(a), id_of(b) };
	BodyLockMultiWrite lock(world->system.GetBodyLockInterface(), ids, 2);
	Body *first = lock.GetBody(0);
	Body *second = lock.GetBody(1);
	if (first == nullptr || second == nullptr) {
		lock.ReleaseLocks();
		if (!anchor.IsInvalid()) {
			world->system.GetBodyInterface().RemoveBody(anchor);
			world->system.GetBodyInterface().DestroyBody(anchor);
		}
		return 0;
	}
	Vec3 along = Vec3(axis[0], axis[1], axis[2]).NormalizedOr(Vec3::sAxisY());
	Ref<TwoBodyConstraintSettings> settings;
	switch (kind) {
	case 0: {
		auto fixed = new FixedConstraintSettings();
		fixed->mAutoDetectPoint = true;
		settings = fixed;
		break;
	}
	case 1: {
		auto pivot = new PointConstraintSettings();
		pivot->mPoint1 = pivot->mPoint2 = at;
		settings = pivot;
		break;
	}
	case 2: {
		auto hinge = new HingeConstraintSettings();
		hinge->mPoint1 = hinge->mPoint2 = at;
		hinge->mHingeAxis1 = hinge->mHingeAxis2 = along;
		hinge->mNormalAxis1 = hinge->mNormalAxis2 = along.GetNormalizedPerpendicular();
		if (min < max) {
			hinge->mLimitsMin = min;
			hinge->mLimitsMax = max;
		}
		settings = hinge;
		break;
	}
	default: {
		// From the first body's centre to the point, held to the second.
		auto distance = new DistanceConstraintSettings();
		distance->mPoint1 = first->GetCenterOfMassPosition();
		distance->mPoint2 = at;
		if (min >= 0.0f && max > 0.0f) {
			distance->mMinDistance = min;
			distance->mMaxDistance = max;
		}
		settings = distance;
		break;
	}
	}
	Ref<Constraint> joint = settings->Create(*first, *second);
	lock.ReleaseLocks();
	world->system.AddConstraint(joint);
	for (size_t at = 0; at < world->joints.size(); ++at) {
		if (world->joints[at] == nullptr) {
			world->joints[at] = joint;
			world->anchors[at] = anchor;
			return uint32_t(at + 1);
		}
	}
	world->joints.push_back(joint);
	world->anchors.push_back(anchor);
	return uint32_t(world->joints.size());
}

void vx_joint_remove(VxWorld *world, uint32_t joint) {
	if (joint > 0 && joint <= world->joints.size() && world->joints[joint - 1] != nullptr) {
		world->system.RemoveConstraint(world->joints[joint - 1]);
		world->joints[joint - 1] = nullptr;
		BodyID anchor = world->anchors[joint - 1];
		if (!anchor.IsInvalid()) {
			world->system.GetBodyInterface().RemoveBody(anchor);
			world->system.GetBodyInterface().DestroyBody(anchor);
			world->anchors[joint - 1] = BodyID();
		}
	}
}

// A wheel, in its vehicle's body's space: where its suspension is fixed,
// how far it travels, and what the wheel is.
struct VxWheelDesc {
	float position[3];
	float radius;
	float width;
	float suspension_min;
	float suspension_max;
	float frequency;
	float damping;
	float max_steer; // radians
	float max_brake; // Nm
	float max_handbrake; // Nm
	uint32_t powered;
	float grip; // Jolt's tyre friction, scaled, forwards
	float side_grip; // and sideways
};

struct VxVehicleDesc {
	uint32_t kind; // 0 a car, 1 a tank, 2 a motorbike
	float max_torque;
	float min_rpm;
	float max_rpm;
	uint32_t automatic;
	float max_pitch_roll; // radians
	uint32_t wheel_count;
	const VxWheelDesc *wheels;
	// Forward gears, the first's ratio and the top's, the rest spread between; 0 for Jolt's own.
	uint32_t gears;
	float first_gear;
	float top_gear;
	float anti_roll; // N/m, between wheels side by side; 0 for none
};

struct VxVehicleState {
	float rpm;
	int32_t gear;
	float speed; // m/s, forward
};

}

namespace {

// A tyre's friction curve, scaled: grip 2 holds twice as hard at every slip.
void scale_friction(LinearCurve &curve, float grip) {
	if (grip <= 0.0f || grip == 1.0f) {
		return;
	}
	for (LinearCurve::Point &point : curve.mPoints) {
		point.mY *= grip;
	}
}

void fill_wheel(WheelSettings &wheel, const VxWheelDesc &desc) {
	wheel.mPosition = Vec3(desc.position[0], desc.position[1], desc.position[2]);
	wheel.mRadius = desc.radius;
	wheel.mWidth = desc.width;
	wheel.mSuspensionMinLength = desc.suspension_min;
	wheel.mSuspensionMaxLength = std::max(desc.suspension_max, desc.suspension_min + 0.01f);
	wheel.mSuspensionSpring.mFrequency = desc.frequency;
	wheel.mSuspensionSpring.mDamping = desc.damping;
}

// A gearbox of `gears` forward gears from `first` to `top`, each ratio the
// same step down from the one before, and a reverse a little lower than
// first, as cars have.
void fill_gears(VehicleTransmissionSettings &transmission, const VxVehicleDesc &desc) {
	if (desc.gears == 0 || desc.first_gear <= 0.0f || desc.top_gear <= 0.0f) {
		return;
	}
	transmission.mGearRatios.clear();
	float step = desc.gears > 1 ? std::pow(desc.top_gear / desc.first_gear, 1.0f / float(desc.gears - 1)) : 1.0f;
	float ratio = desc.first_gear;
	for (uint32_t gear = 0; gear < desc.gears; ++gear) {
		transmission.mGearRatios.push_back(ratio);
		ratio *= step;
	}
	transmission.mReverseGearRatios.clear();
	transmission.mReverseGearRatios.push_back(-desc.first_gear * 1.09f);
}

// Wheels side by side, as an axle's: each pair by where they are along the
// body, nearer than a quarter metre, left (+X in Jolt) first.
std::vector<std::pair<int, int>> pairs_side_by_side(const VxVehicleDesc &desc, bool powered_only) {
	std::vector<uint32_t> wheels;
	for (uint32_t at = 0; at < desc.wheel_count; ++at) {
		if (!powered_only || desc.wheels[at].powered) {
			wheels.push_back(at);
		}
	}
	std::sort(wheels.begin(), wheels.end(), [&](uint32_t a, uint32_t b) { return desc.wheels[a].position[2] < desc.wheels[b].position[2]; });
	std::vector<std::pair<int, int>> axles;
	for (size_t at = 0; at < wheels.size(); ++at) {
		const VxWheelDesc &wheel = desc.wheels[wheels[at]];
		bool paired = at + 1 < wheels.size() && std::abs(desc.wheels[wheels[at + 1]].position[2] - wheel.position[2]) < 0.25f;
		if (paired) {
			axles.emplace_back(int(wheels[at]), int(wheels[at + 1]));
			++at;
		} else {
			axles.emplace_back(int(wheels[at]), -1);
		}
	}
	return axles;
}

// A car's or a motorbike's controller: powered wheels side by side share a
// differential, each with an equal share of the engine.
void fill_wheeled(WheeledVehicleControllerSettings &controller, const VxVehicleDesc &desc) {
	controller.mEngine.mMaxTorque = desc.max_torque;
	controller.mEngine.mMinRPM = desc.min_rpm;
	controller.mEngine.mMaxRPM = std::max(desc.max_rpm, desc.min_rpm + 1.0f);
	controller.mTransmission.mMode = desc.automatic ? ETransmissionMode::Auto : ETransmissionMode::Manual;
	fill_gears(controller.mTransmission, desc);
	std::vector<std::pair<int, int>> axles = pairs_side_by_side(desc, true);
	for (auto [left, right] : axles) {
		VehicleDifferentialSettings differential;
		differential.mLeftWheel = left;
		differential.mRightWheel = right;
		differential.mEngineTorqueRatio = 1.0f / float(axles.size());
		controller.mDifferentials.push_back(differential);
	}
}

} // namespace

extern "C" {

uint32_t vx_vehicle_add(VxWorld *world, uint32_t body, const VxVehicleDesc *desc) {
	BodyLockWrite lock(world->system.GetBodyLockInterface(), id_of(body));
	if (!lock.Succeeded() || desc->wheel_count == 0) {
		return 0;
	}
	Body &vehicle_body = lock.GetBody();
	VehicleConstraintSettings settings;
	settings.mMaxPitchRollAngle = desc->max_pitch_roll;
	// An anti-roll bar across each pair of wheels side by side, to keep the body from leaning out of a turn.
	if (desc->anti_roll > 0.0f) {
		for (auto [left, right] : pairs_side_by_side(*desc, false)) {
			if (right >= 0) {
				VehicleAntiRollBar bar;
				bar.mLeftWheel = left;
				bar.mRightWheel = right;
				bar.mStiffness = desc->anti_roll;
				settings.mAntiRollBars.push_back(bar);
			}
		}
	}
	if (desc->kind == 1) {
		auto controller = new TrackedVehicleControllerSettings();
		controller->mEngine.mMaxTorque = desc->max_torque;
		controller->mEngine.mMinRPM = desc->min_rpm;
		controller->mEngine.mMaxRPM = std::max(desc->max_rpm, desc->min_rpm + 1.0f);
		controller->mTransmission.mMode = desc->automatic ? ETransmissionMode::Auto : ETransmissionMode::Manual;
		fill_gears(controller->mTransmission, *desc);
		// Jolt's left is +X, as it is right-handed; each track takes the wheels on its side.
		for (int side = 0; side < 2; ++side) {
			VehicleTrackSettings &track = controller->mTracks[side];
			track.mWheels.clear();
			bool driven = false;
			for (uint32_t at = 0; at < desc->wheel_count; ++at) {
				bool left = desc->wheels[at].position[0] >= 0.0f;
				if (left != (side == int(ETrackSide::Left))) {
					continue;
				}
				track.mWheels.push_back(at);
				if (!driven && desc->wheels[at].powered) {
					track.mDrivenWheel = at;
					driven = true;
				}
			}
			if (!track.mWheels.empty() && !driven) {
				track.mDrivenWheel = track.mWheels[0];
			}
		}
		settings.mController = controller;
		for (uint32_t at = 0; at < desc->wheel_count; ++at) {
			auto wheel = new WheelSettingsTV();
			fill_wheel(*wheel, desc->wheels[at]);
			settings.mWheels.push_back(wheel);
		}
	} else {
		Ref<WheeledVehicleControllerSettings> controller =
			desc->kind == 2 ? new MotorcycleControllerSettings() : new WheeledVehicleControllerSettings();
		fill_wheeled(*controller, *desc);
		settings.mController = controller;
		for (uint32_t at = 0; at < desc->wheel_count; ++at) {
			auto wheel = new WheelSettingsWV();
			fill_wheel(*wheel, desc->wheels[at]);
			wheel->mMaxSteerAngle = desc->wheels[at].max_steer;
			wheel->mMaxBrakeTorque = desc->wheels[at].max_brake;
			wheel->mMaxHandBrakeTorque = desc->wheels[at].max_handbrake;
			scale_friction(wheel->mLongitudinalFriction, desc->wheels[at].grip);
			scale_friction(wheel->mLateralFriction, desc->wheels[at].side_grip);
			if (desc->kind == 2) {
				// A motorbike's front wheel steers about a raked fork; both lean with the bike.
				wheel->mSteeringAxis = Vec3(0, 1, desc->wheels[at].max_steer > 0.0f ? -0.4f : 0.0f).Normalized();
			}
			settings.mWheels.push_back(wheel);
		}
	}
	Vehicle vehicle;
	vehicle.kind = desc->kind;
	vehicle.constraint = new VehicleConstraint(vehicle_body, settings);
	vehicle.tester = new VehicleCollisionTesterCastCylinder(vehicle_body.GetObjectLayer());
	vehicle.constraint->SetVehicleCollisionTester(vehicle.tester);
	world->system.AddConstraint(vehicle.constraint);
	world->system.AddStepListener(vehicle.constraint);
	for (size_t at = 0; at < world->vehicles.size(); ++at) {
		if (world->vehicles[at].constraint == nullptr) {
			world->vehicles[at] = vehicle;
			return uint32_t(at + 1);
		}
	}
	world->vehicles.push_back(vehicle);
	return uint32_t(world->vehicles.size());
}

static Vehicle *vehicle_of(VxWorld *world, uint32_t vehicle) {
	if (vehicle == 0 || vehicle > world->vehicles.size() || world->vehicles[vehicle - 1].constraint == nullptr) {
		return nullptr;
	}
	return &world->vehicles[vehicle - 1];
}

void vx_vehicle_remove(VxWorld *world, uint32_t vehicle) {
	Vehicle *found = vehicle_of(world, vehicle);
	if (found == nullptr) {
		return;
	}
	world->system.RemoveStepListener(found->constraint);
	world->system.RemoveConstraint(found->constraint);
	*found = Vehicle {};
}

// Drives a vehicle: `forward` from -1 (reverse) to 1, `right` the way it
// steers in Voxear's own sense, from -1 left to 1 right, and the brakes from
// 0 to 1. Jolt's right is Voxear's left, so the steering turns round here.
void vx_vehicle_drive(VxWorld *world, uint32_t vehicle, float forward, float right, float brake, float handbrake) {
	Vehicle *found = vehicle_of(world, vehicle);
	if (found == nullptr) {
		return;
	}
	if (forward != 0.0f || right != 0.0f || brake != 0.0f || handbrake != 0.0f) {
		world->system.GetBodyInterface().ActivateBody(found->constraint->GetVehicleBody()->GetID());
	}
	VehicleController *controller = found->constraint->GetController();
	if (found->kind != 1) {
		static_cast<WheeledVehicleController *>(controller)->SetDriverInput(forward, -right, brake, handbrake);
		return;
	}
	// A tank turns by running its tracks at different speeds, and on the spot by running them opposite ways.
	float steer = std::clamp(right, -1.0f, 1.0f);
	float left_ratio = 1.0f, right_ratio = 1.0f;
	if (std::abs(steer) > 0.01f && std::abs(forward) < 0.1f) {
		forward = 1.0f;
		left_ratio = steer > 0.0f ? -1.0f : 1.0f;
		right_ratio = -left_ratio;
	} else if (steer > 0.0f) {
		left_ratio = std::max(1.0f - 2.0f * steer, 0.05f);
	} else if (steer < 0.0f) {
		right_ratio = std::max(1.0f + 2.0f * steer, 0.05f);
	}
	// Jolt's left track is on +X, Voxear's right: turning right runs it slower.
	static_cast<TrackedVehicleController *>(controller)->SetDriverInput(forward, left_ratio, right_ratio, std::max(brake, handbrake));
}

void vx_vehicle_state(VxWorld *world, uint32_t vehicle, VxVehicleState *out) {
	*out = VxVehicleState {};
	Vehicle *found = vehicle_of(world, vehicle);
	if (found == nullptr) {
		return;
	}
	const VehicleEngine *engine = nullptr;
	const VehicleTransmission *transmission = nullptr;
	VehicleController *controller = found->constraint->GetController();
	if (found->kind == 1) {
		auto tracked = static_cast<TrackedVehicleController *>(controller);
		engine = &tracked->GetEngine();
		transmission = &tracked->GetTransmission();
	} else {
		auto wheeled = static_cast<WheeledVehicleController *>(controller);
		engine = &wheeled->GetEngine();
		transmission = &wheeled->GetTransmission();
	}
	out->rpm = engine->GetCurrentRPM();
	out->gear = transmission->GetCurrentGear();
	const Body *body = found->constraint->GetVehicleBody();
	out->speed = body->GetLinearVelocity().Dot(body->GetRotation() * Vec3::sAxisZ());
}

// Where a wheel is now, and how it is turned: steered, rolled and down on
// its suspension, for its Obj.
void vx_vehicle_wheel(VxWorld *world, uint32_t vehicle, uint32_t wheel, float position[3], float rotation[4]) {
	Vehicle *found = vehicle_of(world, vehicle);
	if (found == nullptr || wheel >= found->constraint->GetWheels().size()) {
		return;
	}
	RMat44 placed = found->constraint->GetWheelWorldTransform(wheel, Vec3::sAxisX(), Vec3::sAxisY());
	RVec3 at = placed.GetTranslation();
	Quat turn = placed.GetQuaternion();
	position[0] = float(at.GetX());
	position[1] = float(at.GetY());
	position[2] = float(at.GetZ());
	rotation[0] = turn.GetX();
	rotation[1] = turn.GetY();
	rotation[2] = turn.GetZ();
	rotation[3] = turn.GetW();
}

} // extern "C"
