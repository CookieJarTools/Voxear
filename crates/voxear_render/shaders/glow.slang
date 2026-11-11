// Glow that moves (src/assets.rs's Glow): a material's emission pulsing,
// breathing or flickering, each Obj or plant at its own moment by where it
// stands, so a field of glowing mushrooms never pulses as one.

#pragma once

#include "common.slang"

float glowHash(float3 p)
{
    return frac(sin(dot(p, float3(12.9898, 78.233, 37.719))) * 43758.5453);
}

// Smooth noise along a line, from 0 to 1.
float glowNoise(float t)
{
    float cell = floor(t);
    float f = t - cell;
    float u = f * f * (3.0 - 2.0 * f);
    return lerp(glowHash(float3(cell, 0.0, 0.0)), glowHash(float3(cell + 1.0, 0.0, 0.0)), u);
}

// How much of its emission a material gives now, on what stands at `origin`.
float glowAmount(Frame* frame, Material material, float3 origin)
{
    uint motion = (material.flags >> MATERIAL_GLOW_SHIFT) & 3u;
    if (motion == 0)
    {
        return 1.0;
    }
    // Each thing its own phase, by where it stands from the level's middle, kept as the world's middle moves.
    float phase = glowHash(floor(levelPosition(frame, origin, 1024.0) * 7.0));
    float t = frame->time * material.glowSpeed + phase;
    float wave = 0.5 + 0.5 * sin(t * 2.0 * PI);
    switch (motion)
    {
    case 1:
        return wave;
    case 2:
        // Long dim rests between slow swells.
        return 0.15 + 0.85 * pow(wave, 3.0);
    default:
        // Two speeds of noise, never quite out.
        float flame = glowNoise(t * 6.0 + phase * 31.0) * 0.6 + glowNoise(t * 17.0 + phase * 53.0) * 0.4;
        return 0.35 + 0.65 * flame;
    }
}
