// Simulated particles (src/particle_sim.rs): the particles of every system
// that needs more than its age to say where a particle is, kept in one pool
// from frame to frame and stepped here, once a frame for every view, among
// the main view's passes, once its depth is drawn. A
// system has a run of the pool to itself and fills it in a ring, each new
// particle taking the place of its oldest, which has died by then; so
// nothing is counted or sorted, and particles are drawn in the order they
// were sent out. The pass steps whole chunks of the pool, only those of
// systems that have particles out.
//
// A new particle may be older than nothing: one sent out part way through
// the frame, or one of a system first seen with its stream already running.
// It starts where its age alone would have it (particleFlown, as a system
// that is not simulated draws its own), then the wind and turbulence take
// it from there.
//
// A system that collides meets what the main view sees: a particle that
// has come within its own radius of the surface under it, by the view's
// depth, is put back on it and bounces off, as much as the system says. What
// the view cannot see, behind it or hidden, stops nothing.
//
// A system that takes forces is moved by the frame's Force Fields and
// blasts (src/forces.rs), and driven out of the way of what passes through
// it, the same pushers that part plants.

#include "particle_motion.slang"
#include "wind.slang"

// How fast a particle takes up the wind's speed, each second, at Wind 1.
static const float WIND_PULL = 2.0;
// How fast turbulence's swirls change, in swirls a second.
static const float TURBULENCE_DRIFT = 0.3;
// How hard what passes through particles (the pushers that part plants) drives them out of its way at its middle,
// in metres a second each second.
static const float PUSHER_FORCE = 40.0;

// Where `place` is from the level's real middle, wrapped as levelPosition wraps it.
float3 simLevelPosition(ParticleSim* sim, float3 place, float period)
{
    float3 at = place + fmod(sim->origin.xyz, period);
    return at - floor(at / period + 0.5) * period;
}

// Three numbers from 0 to 1 for a whole-numbered cell, one hash between them.
float3 cellNumbers(int3 cell)
{
    uint h = particleHash(uint(cell.x) * 73856093u ^ uint(cell.y) * 19349663u ^ uint(cell.z) * 83492791u);
    return float3(float(h & 0x7FFu), float((h >> 11u) & 0x7FFu), float(h >> 22u)) * float3(1.0 / 2047.0, 1.0 / 2047.0, 1.0 / 1023.0);
}

// Which way turbulence stirs the air at `p`, about one unit across a swirl:
// the curl of three smooth noises, so it swirls without gathering particles
// together or thinning them out. About 1 long. The noises are value noise
// between whole-numbered cells, eased so their slopes are smooth too; all
// three are read from the same eight cells, and only their slopes are
// worked out, which is all a curl needs.
float3 swirl(float3 p)
{
    float3 cell = floor(p);
    float3 f = p - cell;
    float3 u = f * f * f * (f * (f * 6.0 - 15.0) + 10.0);
    float3 du = 30.0 * f * f * (f * (f - 2.0) + 1.0);
    int3 c = int3(cell);
    float3 a = cellNumbers(c);
    float3 b = cellNumbers(c + int3(1, 0, 0));
    float3 d = cellNumbers(c + int3(0, 1, 0));
    float3 e = cellNumbers(c + int3(1, 1, 0));
    float3 g = cellNumbers(c + int3(0, 0, 1));
    float3 h = cellNumbers(c + int3(1, 0, 1));
    float3 i = cellNumbers(c + int3(0, 1, 1));
    float3 j = cellNumbers(c + int3(1, 1, 1));
    float3 k1 = b - a;
    float3 k2 = d - a;
    float3 k3 = g - a;
    float3 k4 = a - b - d + e;
    float3 k5 = a - d - g + i;
    float3 k6 = a - b - g + h;
    float3 k7 = -a + b + d - e + g - h - i + j;
    // Each noise's slope along x, along y and along z: in .x the first noise's, in .y the second's, in .z the third's.
    float3 alongX = du.x * (k1 + k4 * u.y + k6 * u.z + k7 * u.y * u.z);
    float3 alongY = du.y * (k2 + k5 * u.z + k4 * u.x + k7 * u.z * u.x);
    float3 alongZ = du.z * (k3 + k6 * u.x + k5 * u.y + k7 * u.x * u.y);
    return float3(alongY.z - alongZ.y, alongZ.x - alongX.z, alongX.y - alongY.x);
}

// Where the scene's surface under `pixel` of the main view is in the world; false where there is none, only sky.
// The view's depth is push.textures.x.
bool surfaceUnder(Frame* frame, int2 pixel, out float3 at)
{
    int2 size = int2(frame->viewport);
    pixel = clamp(pixel, 0, size - 1);
    float depth = textures[NonUniformResourceIndex(push.textures.x)].Load(int3(pixel, 0)).r;
    float2 uv = (float2(pixel) + 0.5) / float2(size);
    float4 world = transform(frame->invViewProj, float4(uv * 2.0 - 1.0, depth, 1.0));
    at = world.xyz / world.w;
    return depth > 0.0;
}

// Which way the surface at `at`, under `pixel`, faces: across to the pixel beside it and the one above or below,
// whichever of each pair is nearer it in the world, so an edge does not tilt it.
float3 surfaceFacing(Frame* frame, int2 pixel, float3 at)
{
    float3 right, left, down, up;
    bool hasRight = surfaceUnder(frame, pixel + int2(1, 0), right);
    bool hasLeft = surfaceUnder(frame, pixel - int2(1, 0), left);
    bool hasDown = surfaceUnder(frame, pixel + int2(0, 1), down);
    bool hasUp = surfaceUnder(frame, pixel - int2(0, 1), up);
    float3 acrossRight = right - at;
    float3 acrossLeft = at - left;
    float3 acrossDown = down - at;
    float3 acrossUp = at - up;
    float3 across = (!hasLeft || (hasRight && dot(acrossRight, acrossRight) < dot(acrossLeft, acrossLeft))) ? acrossRight : acrossLeft;
    float3 along = (!hasUp || (hasDown && dot(acrossDown, acrossDown) < dot(acrossUp, acrossUp))) ? acrossDown : acrossUp;
    float3 facing = normalizeOr(cross(across, along), float3(0.0, 1.0, 0.0));
    return dot(facing, frame->cameraPosition - at) < 0.0 ? -facing : facing;
}

// A particle `radius` wide at `position` in the world, going at `velocity`, against what the main view sees: put
// back on the surface it has come to and sent off it, keeping `bounce` of its speed into it and losing `friction`
// of its speed along it. True when it struck it, rather than rested on it or met nothing.
bool meetScene(Frame* frame, float radius, float bounce, float friction, float step, inout float3 position, inout float3 velocity)
{
    float4 clip = transform(frame->viewProj, float4(position, 1.0));
    if (clip.w <= frame->near)
    {
        return false;
    }
    float2 uv = clip.xy / clip.w * 0.5 + 0.5;
    if (any(uv < 0.0) || any(uv > 1.0))
    {
        return false;
    }
    int2 pixel = int2(uv * float2(frame->viewport));
    float3 surface;
    if (!surfaceUnder(frame, pixel, surface))
    {
        return false;
    }
    // Only near the surface, in front or a little way in: one far behind it is behind a wall, not in it.
    float speed = length(velocity);
    float reach = radius + speed * step * 1.5 + 0.1;
    float3 toSurface = surface - position;
    if (dot(toSurface, toSurface) > reach * reach * 16.0)
    {
        return false;
    }
    float3 facing = surfaceFacing(frame, pixel, surface);
    float above = dot(position - surface, facing);
    if (above >= radius || above < -reach)
    {
        return false;
    }
    position += facing * (radius - above);
    float into = dot(velocity, facing);
    if (into >= 0.0)
    {
        return false;
    }
    velocity -= facing * into * (1.0 + bounce);
    float3 sliding = velocity - facing * dot(velocity, facing);
    velocity -= sliding * friction;
    // Falling onto it for a frame, as one resting on it does, is no strike.
    return -into > PARTICLE_GRAVITY * step * 2.0 + 0.1;
}

// How the frame's forces speed up a particle at `at` in the world, in metres a second each second: each Force Field
// and blast within reach, the less the further out; and what passes through, driving it out of its way.
float3 forcesAt(ParticleSim* sim, Frame* frame, float3 at)
{
    float3 pushed = 0.0;
    for (uint i = 0; i < sim->forceCount; i++)
    {
        Force force = sim->forces[i];
        float3 from = at - force.position;
        float out2 = dot(from, from);
        if (out2 >= force.radius * force.radius)
        {
            continue;
        }
        float out = sqrt(out2);
        float near = 1.0 - out / force.radius;
        float hard = force.strength * near * near;
        float3 away = out > 1e-4 ? from / out : float3(0.0, 1.0, 0.0);
        if (force.kind == FORCE_ATTRACT)
        {
            pushed -= away * hard;
        }
        else if (force.kind == FORCE_REPEL)
        {
            pushed += away * hard;
        }
        else if (force.kind == FORCE_VORTEX)
        {
            // Round its axis, and in toward it by as much, so they circle it rather than fly off.
            float3 fromAxis = from - force.axis * dot(from, force.axis);
            float off = length(fromAxis);
            float3 outward = off > 1e-4 ? fromAxis / off : 0.0;
            pushed += (cross(force.axis, outward) - outward * 0.5) * hard;
        }
        else
        {
            pushed += force.axis * hard;
        }
    }
    Pushes pushes = frame->pushes;
    for (uint p = 0; p < pushes.count; p++)
    {
        float4 pusher = pushes.pushers[p];
        float3 from = at - pusher.xyz;
        float out = length(from);
        if (out < pusher.w && out > 1e-4)
        {
            float strength = pushes.strengths[p / 4][p % 4];
            pushed += from / out * (PUSHER_FORCE * strength * (1.0 - out / pusher.w));
        }
    }
    return pushed;
}

[shader("compute")]
[numthreads(64, 1, 1)]
void simulateMain(uint3 id : SV_DispatchThreadID)
{
    ParticleSim* sim = buffers()->particleSim;
    uint chunk = id.x / PARTICLE_CHUNK;
    if (chunk >= sim->chunks)
    {
        return;
    }
    uint2 work = buffers()->particleChunks[chunk];
    ParticleSystem* system = &buffers()->particleSteps[work.x];
    uint slot = work.y + id.x % PARTICLE_CHUNK;
    uint capacity = system->capacity;
    if (slot >= capacity)
    {
        return;
    }
    Particle* kept = &buffers()->particles[system->poolFirst + slot];
    uint flags = system->flags;
    bool inWorld = (flags & PARTICLES_WORLD) != 0;
    float3 pull = float3(0.0, -PARTICLE_GRAVITY * system->gravity, 0.0);
    if (!inWorld)
    {
        pull = transform(system->toLocal, float4(pull, 0.0)).xyz;
    }
    float drag = system->drag;
    uint speedKeys = system->curveCounts.z;

    Particle particle;
    // How long this step moves it for: since last frame, or for a new one, nothing, as it starts where its age has it.
    float step = sim->deltaTime;
    uint since = (slot + capacity - system->head) % capacity;
    if (since < system->emitCount)
    {
        // A new one, taking the place of the system's oldest.
        uint number = system->emitted + since;
        uint h = particleHash(system->seed ^ particleHash(number * 2654435761u + 0x7F4A7C15u));
        particle.seed = h;
        particle.life = lerp(system->lifetime.x, system->lifetime.y, particleRandom(h));
        float3 from;
        float3 way;
        particleSetOut(system->shape, system->radius, system->angle, system->boxSize, h, from, way);
        float3 velocity = way * lerp(system->speed.x, system->speed.y, particleRandom(h));
        // The burst all at once; the stream spread evenly over the time it was sent out in.
        float age = system->burstAge;
        if (since >= system->emitBurst)
        {
            uint stream = system->emitCount - system->emitBurst;
            age = lerp(system->streamAges.x, system->streamAges.y, (float(since - system->emitBurst) + 0.5) / float(stream));
        }
        float3 start = from;
        if (inWorld)
        {
            // From where its Obj was when it set out, along the way the Obj has come this frame.
            float back = sim->deltaTime > 1e-6 ? saturate(age / sim->deltaTime) : 0.0;
            start = transform(system->model, float4(from, 1.0)).xyz - system->moved * back;
            velocity = transform(system->model, float4(velocity, 0.0)).xyz;
        }
        particle.age = age;
        particle.position = particleFlown(pull, drag, system->speedCurve, speedKeys, start, velocity, age, particle.life);
        particle.velocity = particleSpeedAfter(pull, drag, velocity, age);
        step = 0.0;
    }
    else
    {
        particle = *kept;
        if ((flags & PARTICLES_FRESH) != 0)
        {
            // Its room held another system's particles, or its own from before it started over.
            particle.life = 0.0;
            *kept = particle;
            return;
        }
        if (particle.life <= 0.0 || particle.age >= particle.life)
        {
            return;
        }
        particle.age += step;
        if (inWorld)
        {
            particle.position += system->shift;
        }
    }

    if (step > 0.0 && particle.age < particle.life)
    {
        float3 velocity = particle.velocity + pull * step;
        float3 world = inWorld ? particle.position : transform(system->model, float4(particle.position, 1.0)).xyz;
        float3 carried = 0.0;
        float wind = system->wind;
        if (wind > 0.0 && sim->wind.strength > 0.0)
        {
            // Taken up by the air: toward the wind's speed, the faster the further from it.
            float3 air = windVelocity(sim->wind, simLevelPosition(sim, world, 4096.0).xz, sim->time) * wind;
            if (!inWorld)
            {
                air = transform(system->toLocal, float4(air, 0.0)).xyz;
            }
            velocity += (air - velocity) * (1.0 - exp(-WIND_PULL * step));
        }
        float taken = system->forces;
        if (taken > 0.0)
        {
            float3 pushed = forcesAt(sim, push.frame, world) * taken;
            if (!inWorld)
            {
                pushed = transform(system->toLocal, float4(pushed, 0.0)).xyz;
            }
            velocity += pushed * step;
        }
        float turbulence = system->turbulence;
        if (turbulence > 0.0)
        {
            // Stirred as it goes, without gaining speed of its own from it.
            float size = max(system->turbulenceSize, 0.01);
            float3 at = simLevelPosition(sim, world, 4096.0) / size + float3(0.0, -sim->time * TURBULENCE_DRIFT, 0.0);
            carried = swirl(at) * turbulence;
            if (!inWorld)
            {
                carried = transform(system->toLocal, float4(carried, 0.0)).xyz;
            }
        }
        velocity *= exp(-drag * step);
        float pace = speedKeys > 0 ? curveAt(system->speedCurve, speedKeys, particle.age / particle.life) : 1.0;
        particle.position += (velocity * pace + carried) * step;
        if ((flags & PARTICLES_COLLIDE) != 0)
        {
            // In the world, where the view's depth is; then back to where the particle is kept.
            float3 at = particle.position;
            float3 going = velocity;
            if (!inWorld)
            {
                at = transform(system->model, float4(at, 1.0)).xyz;
                going = transform(system->model, float4(going, 0.0)).xyz;
            }
            float radius = (system->size.x + system->size.y) * 0.25;
            if (meetScene(push.frame, radius, system->bounce, system->friction, step, at, going))
            {
                particle.life = particle.age + (particle.life - particle.age) * (1.0 - system->lifeLost);
            }
            if (!inWorld)
            {
                at = transform(system->toLocal, float4(at, 1.0)).xyz;
                going = transform(system->toLocal, float4(going, 0.0)).xyz;
            }
            particle.position = at;
            velocity = going;
        }
        particle.velocity = velocity;
    }
    *kept = particle;

    bool alive = particle.age < particle.life;
    uint lanes = WaveActiveCountBits(alive);
    if (WaveIsFirstLane() && lanes > 0)
    {
        uint ignored;
        InterlockedAdd(buffers()->particleCounters[0], lanes, ignored);
    }
}
