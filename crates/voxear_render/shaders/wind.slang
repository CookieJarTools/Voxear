// Plants moving in the level's wind (see src/wind.rs): every pass that
// places a vertex moves it by windOffset, so a plant's shading, depth,
// shadow and motion all move together.
//
// A plant bends from the ground up, as far as its height above its
// instance's origin is up its whole height, so its pivot belongs at its
// base. It leans downwind as hard as the wind and the gust over it blow,
// sways back and forth and across the wind by the turbulence, and its
// leaves flutter along their own faces. Gusts are noise drifting downwind
// across the level, so neighbouring plants move together and a field shows
// waves going through it.

#pragma once

#include "common.slang"

// How far the tip of a plant leans at strength 1, as a share of its height.
static const float WIND_LEAN = 0.08;
// How far leaves flutter at strength 1, in metres.
static const float WIND_FLUTTER = 0.02;

float windHash(float2 p)
{
    return frac(sin(dot(p, float2(127.1, 311.7))) * 43758.5453);
}

// Smooth noise from 0 to 1, varying over about one unit.
float windNoise(float2 p)
{
    float2 cell = floor(p);
    float2 f = p - cell;
    float2 u = f * f * (3.0 - 2.0 * f);
    float a = windHash(cell);
    float b = windHash(cell + float2(1.0, 0.0));
    float c = windHash(cell + float2(0.0, 1.0));
    float d = windHash(cell + float2(1.0, 1.0));
    return lerp(lerp(a, b, u.x), lerp(c, d, u.x), u.y);
}

// How tall a plant is: the top of its mesh's bounds above its origin, as its instance scales it.
float plantHeight(Instance instance, Mesh mesh)
{
    return max((mesh.center.y + mesh.radius) * length(instance.model.c1.xyz), 0.05);
}

// How far the wind moves a vertex at `world` of a plant standing at
// `origin`, `height` tall, drawn with `material`, its normal `normal`, at
// `time`.
float3 windOffset(Frame* frame, Material material, float3 origin, float height, float3 world, float3 normal, float time)
{
    Wind wind = frame->wind;
    if (wind.strength <= 0.0 || (material.windBend <= 0.0 && material.windFlutter <= 0.0))
    {
        return 0.0;
    }
    float3 along = float3(wind.direction.x, 0.0, wind.direction.y);
    float3 across = float3(-wind.direction.y, 0.0, wind.direction.x);
    // The gust over the plant: noise drifting downwind, so it sweeps across the level.
    float2 drift = origin.xz / wind.gustSize - wind.direction * (time * wind.gustSpeed / wind.gustSize);
    float gust = windNoise(drift) * 0.7 + windNoise(drift * 2.3 + 17.0) * 0.3;
    float push = wind.strength * (1.0 + wind.gusts * (gust * 2.0 - 0.6));
    // Each plant sways at its own phase, so they never move in step.
    float phase = windHash(floor(origin.xz * 4.0)) * 6.2831;
    float sway = sin(time * 1.6 + phase) * 0.6 + sin(time * 2.7 + phase * 1.7) * 0.4;
    float rock = sin(time * 2.1 + phase * 2.3);

    // Bent from the ground up: nothing at the root, the most at the tip.
    float up = saturate((world.y - origin.y) / height);
    float lean = material.windBend * up * up * height * WIND_LEAN;
    float3 offset = along * lean * max(push + wind.turbulence * wind.strength * sway * 0.5, 0.0);
    offset += across * lean * wind.turbulence * wind.strength * rock * 0.35;
    // The tip moves on an arc, lowering as it leans, so the plant keeps its length.
    offset.y -= dot(offset.xz, offset.xz) / (2.0 * height);

    // Leaves flutter along their faces, quickly and a little, more out at the tips.
    float flicker = dot(world, float3(3.1, 2.3, 4.7));
    float flutter = sin(time * 13.0 + flicker) * 0.6 + sin(time * 21.0 + flicker * 1.7) * 0.4;
    offset += normal * flutter * material.windFlutter * WIND_FLUTTER * max(push, 0.0) * (0.3 + 0.7 * up);
    return offset;
}
