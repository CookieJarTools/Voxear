// Reflection probes as shading reads them (src/reflection_probes.rs keeps
// them): each a cube map of what can be seen from a point in the level,
// captured on the GPU, with a mip for each roughness, filtered by GGX. A
// probe reflects inside its box; with box projection, a ray's direction is
// bent to where it would meet the box, so a room's probe lines up with its
// walls from anywhere inside it rather than only from where it was
// captured. Outside the box, it fades out over its blend distance.
//
// A reflection that finds nothing of its own, on screen or traced, takes the
// probes round it; what they leave, the GI's probes, from where the ray got
// to; and what they leave, the sky.

#pragma once

#include "common.slang"
#include "atmosphere.slang"
#include "ddgi_sample.slang"

// The sky in a direction, as a surface reflects it: the atmosphere, or the
// plain sky's colours, scaled as the sky's other light is, so a night
// scene's floor does not mirror a day sky.
float3 reflectedSky(Frame* frame, float3 direction, bool atmosphere)
{
    float3 sky;
    if (atmosphere)
    {
        sky = skyToward(frame, direction) * skyLook(frame);
    }
    else
    {
        float up = direction.y;
        float3 above = lerp(frame->skyHorizon, frame->skyZenith, pow(saturate(up), 0.45));
        float3 below = lerp(frame->skyHorizon, frame->groundColor, pow(saturate(-up), 0.35));
        sky = up >= 0.0 ? above : below;
    }
    return sky * frame->ambient;
}

// The direction a cube map's texel looks along, from its face (+x, -x, +y,
// -y, +z, -z) and its place on the face from 0 to 1, as the GPU lays cube
// maps out.
float3 probeFaceDirection(uint face, float2 uv)
{
    float2 p = uv * 2.0 - 1.0;
    switch (face)
    {
    case 0: return float3(1.0, -p.y, -p.x);
    case 1: return float3(-1.0, -p.y, p.x);
    case 2: return float3(p.x, 1.0, p.y);
    case 3: return float3(p.x, -1.0, -p.y);
    case 4: return float3(p.x, -p.y, 1.0);
    default: return float3(-p.x, -p.y, -1.0);
    }
}

// How rough a probe's mip is filtered for: the square of how far down the
// mips it is, so the first few, which nearly smooth surfaces read, stay
// sharp. Spaced evenly, a near mirror read a tenth of a mip as blurred as
// roughness 0.2, which showed as a soft halo beside every edge it mirrored.
float probeMipRoughness(float mip)
{
    float down = mip / float(REFLECTION_PROBE_MIPS - 1);
    return down * down;
}

// The mip of a probe's cube that is as rough as a surface: probeMipRoughness
// turned round.
float probeMip(float roughness)
{
    return sqrt(saturate(roughness)) * float(REFLECTION_PROBE_MIPS - 1);
}

// Where in a probe's box a point is, in metres along the box's own axes from
// its middle.
float3 probeLocal(ReflectionProbe probe, float3 point)
{
    float3 offset = point - probe.centre;
    return float3(dot(offset, probe.axisX), dot(offset, probe.axisY), dot(offset, probe.axisZ));
}

// How much of a probe shows at a point: all of it inside its box, and on
// its sides, as a room's floor and walls are, fading to nothing its blend
// distance outside.
float probeWeight(ReflectionProbe probe, float3 local)
{
    // A few centimetres' grace, as a floor on the box's bottom side, its place found again from the depth, can
    // come out a hair below it.
    static const float GRACE = 0.05;
    float outside = length(max(abs(local) - probe.halfSize - GRACE, 0.0));
    if (outside <= 0.0)
    {
        return 1.0;
    }
    return probe.blend > 0.0 ? saturate(1.0 - outside / probe.blend) : 0.0;
}

// The direction to look up a probe's cube in, for a ray from `local` (as
// probeLocal) along `direction`: with box projection, from where the probe
// took its picture to where the ray leaves its box.
float3 probeLookup(ReflectionProbe probe, float3 local, float3 direction)
{
    if ((probe.flags & REFLECTION_PROBE_BOX_PROJECTION) == 0)
    {
        return direction;
    }
    // From outside, where it blends in, as from the nearest point on the box.
    local = clamp(local, -probe.halfSize, probe.halfSize);
    float3 along = float3(dot(direction, probe.axisX), dot(direction, probe.axisY), dot(direction, probe.axisZ));
    float3 safe = select(abs(along) < 1e-6, float3(1e-6), along);
    float3 far = (sign(safe) * probe.halfSize - local) / safe;
    float leaves = max(min(far.x, min(far.y, far.z)), 0.0);
    float3 hit = local + along * leaves - probeLocal(probe, probe.capture);
    return probe.axisX * hit.x + probe.axisY * hit.y + probe.axisZ * hit.z;
}

// The light the reflection probes round `position` show along `direction`,
// read from the mip as rough as the surface, already weighted, and in
// `left` how much of it they leave to whatever reflects there without them.
// Probes come in order (src/reflection_probes.rs sorts them), each taking
// its share of what those before it left.
float3 probeReflection(Frame* frame, float3 position, float3 direction, float roughness, out float left)
{
    left = 1.0;
    ReflectionProbes probes = frame->reflectionProbes;
    if (probes.count == 0)
    {
        return 0.0;
    }
    float mip = probeMip(roughness);
    float3 light = 0.0;
    for (uint i = 0; i < probes.count && left > 0.004; i++)
    {
        ReflectionProbe probe = buffers()->reflectionProbes[i];
        float3 local = probeLocal(probe, position);
        float weight = probeWeight(probe, local);
        if (weight <= 0.0)
        {
            continue;
        }
        float3 lookup = probeLookup(probe, local, direction);
        float4 at = float4(lookup, float(probe.slot));
        float3 seen = cubeArrays[NonUniformResourceIndex(probe.cubes)].SampleLevel(samplers[SAMPLER_LINEAR_CLAMP], at, mip).rgb;
        light += seen * probe.intensity * weight * left;
        left *= 1.0 - weight;
    }
    return light;
}

// The light from all round a surface facing `normal`, as the reflection
// passes guess it for a rough surface's reflection to blur towards: the
// GI's, looked up at `outside`, a little off the surface, or the sky's
// towards the normal.
float3 reflectionAmbient(Frame* frame, float3 outside, float3 normal, bool atmosphere)
{
    float3 sky = reflectedSky(frame, normal, atmosphere);
    return frame->ddgi.enabled != 0 ? ddgiIrradianceAlong(frame, outside, normal, normal, sky) : sky;
}

// What a reflection takes where it finds nothing of its own: the reflection
// probes round the surface at `position`, then for what they leave, given
// `gi`, the GI's probes looked up at `end`, as far as the ray got before it
// stopped, from the side `away` it came from, then the sky. A ray traced
// through the scene that met nothing went out to the sky, so it takes no
// light from the GI's probes. The rougher the surface, the more the GI's and
// sky's share turns towards `ambient`, the light from all round.
float3 reflectionFallback(Frame* frame, float3 position, float3 end, float3 away, float3 direction, float roughness, float3 ambient, bool atmosphere, bool gi)
{
    float left;
    float3 probes = probeReflection(frame, position, direction, roughness, left);
    if (left <= 0.004)
    {
        return probes;
    }
    float blur = saturate(roughness * 1.5);
    float3 rest = ambient;
    if (blur < 1.0)
    {
        float3 sky = reflectedSky(frame, direction, atmosphere);
        if (gi && frame->ddgi.enabled != 0)
        {
            sky = ddgiIrradianceAlong(frame, end, direction, away, sky);
        }
        rest = lerp(sky, ambient, blur);
    }
    return probes + rest * left;
}
