// What the particles' drawing (particle_shared.slang) and their simulation
// (particle_sim.slang) share, none of it reading the frame: a system's
// flags and shapes, a particle's random numbers, curves and gradients, where
// a particle sets out from and where it has flown to.

#pragma once
#include "common.slang"

// A system's flags (particles::LOOPING and the rest).
static const uint PARTICLES_LOOPING = 1;
static const uint PARTICLES_ADDITIVE = 2;
static const uint PARTICLES_LIT = 4;
// Simulated (src/particle_sim.rs): its particles are kept in the pool, in the world rather than in its Obj's own
// space with PARTICLES_WORLD; and, for the pass that steps them, PARTICLES_FRESH when its room holds none of its
// own yet.
static const uint PARTICLES_SIMULATED = 8;
static const uint PARTICLES_WORLD = 16;
static const uint PARTICLES_FRESH = 32;
// Its particles meet what the main view sees, by its depth.
static const uint PARTICLES_COLLIDE = 64;
// Each particle plays through its texture's pictures over its life, rather than keeping one of them.
static const uint PARTICLES_ANIMATE = 128;

// Its shapes, as particles::EmitShape.
static const uint EMIT_POINT = 0;
static const uint EMIT_SPHERE = 1;
static const uint EMIT_CONE = 2;
static const uint EMIT_BOX = 3;

// Gravity, in metres a second squared.
static const float PARTICLE_GRAVITY = 9.81;

// A well-mixed number from `v` (PCG).
uint particleHash(uint v)
{
    uint state = v * 747796405u + 2891336453u;
    uint word = ((state >> ((state >> 28u) + 4u)) ^ state) * 277803737u;
    return (word >> 22u) ^ word;
}

// The next of a particle's random numbers, from 0 to 1.
float particleRandom(inout uint h)
{
    h = particleHash(h);
    return float(h) / 4294967296.0;
}

// A direction picked evenly from every way.
float3 anyWay(inout uint h)
{
    float z = particleRandom(h) * 2.0 - 1.0;
    float turn = particleRandom(h) * 2.0 * PI;
    float across = sqrt(max(1.0 - z * z, 0.0));
    return float3(across * cos(turn), z, across * sin(turn));
}

// Key `i` of a curve kept two keys to a row: where, then what.
float2 curveKey(float4 rows[4], uint i)
{
    float4 row = rows[i / 2];
    return i % 2 == 0 ? row.xy : row.zw;
}

// A curve's value at `t`, straight between its `count` keys and flat past
// them (voxear_core::curve::Curve::at).
float curveAt(float4 rows[4], uint count, float t)
{
    float2 last = curveKey(rows, 0);
    if (t <= last.x)
    {
        return last.y;
    }
    for (uint i = 1; i < count; i++)
    {
        float2 next = curveKey(rows, i);
        if (t <= next.x)
        {
            return lerp(last.y, next.y, next.x > last.x ? (t - last.x) / (next.x - last.x) : 1.0);
        }
        last = next;
    }
    return last.y;
}

// What a curve adds up to from 0 to `t`, the area under it
// (voxear_core::curve::Curve::sum_to).
float curveSum(float4 rows[4], uint count, float t)
{
    t = saturate(t);
    float2 first = curveKey(rows, 0);
    float total = first.y * min(first.x, t);
    float2 last = first;
    for (uint i = 1; i < count; i++)
    {
        float2 next = curveKey(rows, i);
        if (t <= last.x)
        {
            break;
        }
        float end = min(t, next.x);
        float atEnd = next.x > last.x ? lerp(last.y, next.y, (end - last.x) / (next.x - last.x)) : next.y;
        total += (last.y + atEnd) * 0.5 * (end - last.x);
        last = next;
    }
    return total + last.y * max(t - last.x, 0.0);
}

// A gradient's colour at `t` (voxear_core::curve::Gradient::at).
float3 gradientAt(float4 rows[8], uint count, float t)
{
    float4 last = rows[0];
    if (t <= last.x)
    {
        return last.yzw;
    }
    for (uint i = 1; i < count; i++)
    {
        float4 next = rows[i];
        if (t <= next.x)
        {
            return lerp(last.yzw, next.yzw, next.x > last.x ? (t - last.x) / (next.x - last.x) : 1.0);
        }
        last = next;
    }
    return last.yzw;
}

// Where a particle that set out from `start` at `velocity` is `age`
// seconds into a life of `life`, slowed by the air `drag` each second and
// pulled by `pull`: x = x0 + v0 (1 - e^-kt) / k + g (t / k - (1 - e^-kt) / k^2).
// Its speed over life, when it has keys, scales how far it has flown, added
// up along the way, and slowed by the air as much on average.
float3 particleFlown(float3 pull, float drag, float4 speedCurve[4], uint speedKeys, float3 start, float3 velocity, float age, float life)
{
    float k = drag;
    float slowed = k > 1e-4 ? (1.0 - exp(-k * age)) / k : age;
    float flown = slowed;
    if (speedKeys > 0)
    {
        flown = curveSum(speedCurve, speedKeys, age / life) * life * (age > 1e-5 ? slowed / age : 1.0);
    }
    float3 fallen = k > 1e-4 ? pull * (age - slowed) / k : pull * (0.5 * age * age);
    return start + velocity * flown + fallen;
}

// How fast such a particle goes by then, before its speed over life: v0 e^-kt + g (1 - e^-kt) / k.
float3 particleSpeedAfter(float3 pull, float drag, float3 velocity, float age)
{
    float kept = exp(-drag * age);
    return velocity * kept + (drag > 1e-4 ? pull * (1.0 - kept) / drag : pull * age);
}

// The same for a particle of `system`, pulled down as hard as its gravity says.
float3 particleFlight(ParticleSystem system, float3 start, float3 velocity, float age, float life)
{
    float3 pull = float3(0.0, -PARTICLE_GRAVITY * system.gravity, 0.0);
    return particleFlown(pull, system.drag, system.speedCurve, system.curveCounts.z, start, velocity, age, life);
}

// Where a particle sets out from in its Obj's own space, and which way,
// from a shape of `radius`, `angle` and `boxSize`, taking its random
// numbers from `h`.
void particleSetOut(uint shape, float radius, float angle, float3 boxSize, inout uint h, out float3 from, out float3 way)
{
    from = 0.0;
    way = float3(0.0, 1.0, 0.0);
    if (shape == EMIT_POINT)
    {
        way = anyWay(h);
    }
    else if (shape == EMIT_SPHERE)
    {
        way = anyWay(h);
        from = way * radius * pow(particleRandom(h), 1.0 / 3.0);
    }
    else if (shape == EMIT_CONE)
    {
        float tilt = acos(lerp(1.0, cos(angle), particleRandom(h)));
        float turn = particleRandom(h) * 2.0 * PI;
        way = float3(sin(tilt) * cos(turn), cos(tilt), sin(tilt) * sin(turn));
        float reach = radius * sqrt(particleRandom(h));
        float around = particleRandom(h) * 2.0 * PI;
        from = float3(cos(around), 0.0, sin(around)) * reach;
    }
    else
    {
        from = (float3(particleRandom(h), particleRandom(h), particleRandom(h)) - 0.5) * boxSize;
    }
}
