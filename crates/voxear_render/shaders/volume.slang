// The volumetric fog's froxel volume as the passes that read it see it:
// 128×72 froxels across the screen and 64 slices deep, laid side by side in
// one image, slice after slice. Slices are spaced by the square of their
// number, so more lie close to the camera, where detail shows.

#pragma once

#include "atmosphere.slang"
#include "common.slang"
#include "ddgi_sample.slang"

static const float VOLUME_WIDTH = 128.0;
static const float VOLUME_HEIGHT = 72.0;
static const float VOLUME_SLICES = 64.0;

// The view depth, in metres, at a slice, which may be fractional.
float volumeSliceDepth(float slice, float far)
{
    float t = slice / VOLUME_SLICES;
    return far * t * t;
}

// The slice, fractional, at a view depth.
float volumeSliceAt(float depth, float far)
{
    return sqrt(saturate(depth / far)) * VOLUME_SLICES;
}

// One texel of the volume, by froxel.
float4 volumeLoad(uint volume, int2 cell, int slice)
{
    int2 size = int2(VOLUME_WIDTH, VOLUME_HEIGHT);
    int2 inside = clamp(cell, int2(0), size - 1);
    int at = clamp(slice, 0, int(VOLUME_SLICES) - 1);
    return textures[NonUniformResourceIndex(volume)].Load(int3(inside.x + at * int(VOLUME_WIDTH), inside.y, 0));
}

// The volume at a screen position and a fractional slice, blended across
// the froxels round it and the two slices either side.
float4 volumeSample(uint volume, float2 uv, float slice)
{
    float clamped = clamp(slice, 0.0, VOLUME_SLICES - 1.0);
    float first = floor(clamped);
    float second = min(first + 1.0, VOLUME_SLICES - 1.0);
    float2 inSlice = clamp(uv * float2(VOLUME_WIDTH, VOLUME_HEIGHT), 0.5, float2(VOLUME_WIDTH, VOLUME_HEIGHT) - 0.5);
    float width = VOLUME_WIDTH * VOLUME_SLICES;
    Texture2D table = textures[NonUniformResourceIndex(volume)];
    SamplerState clampedSampler = samplers[SAMPLER_LINEAR_CLAMP];
    float4 a = table.SampleLevel(clampedSampler, float2((first * VOLUME_WIDTH + inSlice.x) / width, inSlice.y / VOLUME_HEIGHT), 0);
    float4 b = table.SampleLevel(clampedSampler, float2((second * VOLUME_WIDTH + inSlice.x) / width, inSlice.y / VOLUME_HEIGHT), 0);
    return lerp(a, b, clamped - first);
}

// What the fog adds between the camera and a view depth, and how much light
// it lets through, in alpha, from the integrated volume. Each of its slices
// holds the fog up to that slice's far side.
float4 volumeAt(uint integrated, float2 uv, float depth, float far)
{
    float slice = volumeSliceAt(depth, far);
    float4 first = volumeSample(integrated, uv, 0.0);
    if (slice < 1.0)
    {
        return lerp(float4(0.0, 0.0, 0.0, 1.0), first, slice);
    }
    return volumeSample(integrated, uv, slice - 1.0);
}

// ---------------------------------------------------------------- Height fog

// Light's brightness as the eye sees it, by colour.
static const float3 FOG_LUMA = float3(0.2126, 0.7152, 0.0722);

// The share of light exponential height fog lets through along a ray from
// the camera, where the fog's density at the camera is `density`.
float heightFogThrough(float density, float falloff, float3 ray)
{
    float rise = falloff * ray.y;
    float heightTerm = abs(rise) > 1e-4 ? (1.0 - exp(-rise)) / rise : 1.0;
    return exp(-density * length(ray) * heightTerm);
}

// How lit the fog is between the camera and a point: with GI, dim where
// walls keep out the sky, judged at both ends of the ray; otherwise 1.
float fogOpenness(Frame* frame, float3 point)
{
    if (frame->ddgi.enabled == 0)
    {
        return 1.0;
    }
    // The open sky's light as the probes count it, from the atmosphere or the gradient.
    float3 sky = frame->lighting.sky == SKY_ATMOSPHERE ? atmosphereAmbient(frame, 0.5, sunThroughAtmosphere(frame))
        : lerp(frame->groundColor, lerp(frame->skyHorizon, frame->skyZenith, 0.5), 0.5) * 0.35 * frame->ambient;
    float open = max(dot(sky, FOG_LUMA), 1e-5);
    float3 back = normalize(frame->cameraPosition - point);
    float atCamera = dot(ddgiLightAround(frame, frame->cameraPosition, back, sky), FOG_LUMA) / open;
    float atPoint = dot(ddgiLightAround(frame, point, back, sky), FOG_LUMA) / open;
    return saturate((atCamera + atPoint) * 0.5);
}

// The colour the mist takes looking along `direction`. Under the atmosphere,
// the sky's own light at the horizon that way, so the mist darkens with the
// night and reddens at dusk, tinted by the fog's colour; under a gradient
// sky, the fog's colour, which the scene's sky colours already set.
float3 fogColourToward(Frame* frame, float3 direction)
{
    if (frame->lighting.sky != SKY_ATMOSPHERE)
    {
        return frame->post.fogColor;
    }
    float3 level = normalize(float3(direction.x, 0.0, direction.z) + float3(0.0, 0.0, 1e-4));
    float3 tint = frame->post.fogColor / max(dot(frame->post.fogColor, FOG_LUMA), 1e-4);
    return skyToward(frame, level) * skyLook(frame) * frame->ambient * tint;
}
