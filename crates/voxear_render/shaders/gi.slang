// Voxel global illumination as shading reads it: light bounced off nearby
// surfaces, and the sky's light where nothing hides it, found by tracing a
// few cones through a clipmap of voxels round the camera (gi_inject.slang
// fills them). Three nested grids of 64³ voxels, each twice the last's size,
// laid out slice after slice across one image, a grid to a band of rows.
// Shading reads last frame's finished voxels, at last frame's origins.

#pragma once

#include "common.slang"

static const float GI_ATLAS_WIDTH = float(GI_SIZE * GI_SIZE);
static const float GI_ATLAS_HEIGHT = float(GI_SIZE * GI_LEVELS);

float giVoxelSize(Frame* frame, uint level)
{
    return frame->gi.voxelSize * float(1u << level);
}

// The voxels of a grid at a point, blended between the voxels round it, as
// last frame left them; nothing outside the grid.
float4 giSample(Frame* frame, float3 position, uint level)
{
    float3 inGrid = position / giVoxelSize(frame, level) - float3(frame->gi.previousOrigin[level].xyz) - 0.5;
    if (any(inGrid < 0.0) || any(inGrid > float(GI_SIZE - 1)))
    {
        return 0.0;
    }
    float first = floor(inGrid.z);
    float second = min(first + 1.0, float(GI_SIZE - 1));
    float2 inSlice = inGrid.xy + 0.5;
    float row = (inSlice.y + float(level * GI_SIZE)) / GI_ATLAS_HEIGHT;
    Texture2D voxels = textures[NonUniformResourceIndex(frame->gi.texture)];
    SamplerState linear = samplers[SAMPLER_LINEAR_CLAMP];
    float4 a = voxels.SampleLevel(linear, float2((inSlice.x + first * float(GI_SIZE)) / GI_ATLAS_WIDTH, row), 0.0);
    float4 b = voxels.SampleLevel(linear, float2((inSlice.x + second * float(GI_SIZE)) / GI_ATLAS_WIDTH, row), 0.0);
    return lerp(a, b, inGrid.z - first);
}

// One cone from a surface: the light the voxels along it send back, and how
// much of the cone they block, widening with distance into coarser grids.
float4 giCone(Frame* frame, float3 start, float3 direction, float tanHalfAngle)
{
    float smallest = frame->gi.voxelSize;
    // Once wider than the coarsest voxels, a cone carries on through the coarsest grid, to half its width.
    float coarsest = float(GI_LEVELS - 1);
    float farthest = smallest * float(1u << (GI_LEVELS - 1)) * float(GI_SIZE / 2);
    float distance = smallest;
    float3 light = 0.0;
    float blocked = 0.0;
    for (uint step = 0; step < 16 && blocked < 0.95 && distance < farthest; step++)
    {
        float width = max(smallest, 2.0 * tanHalfAngle * distance);
        float level = min(ceil(log2(width / smallest)), coarsest);
        float4 voxel = giSample(frame, start + direction * distance, uint(level));
        light += (1.0 - blocked) * voxel.a * voxel.rgb;
        blocked += (1.0 - blocked) * voxel.a;
        // A step as long as the cone is wide, so each sample covers fresh space.
        distance += width;
    }
    return float4(light, blocked);
}

// The sky's light from a direction, between the light from straight up, at
// the horizon and from straight down.
float3 giSky(float3 direction, float3 skyUp, float3 skyHorizon, float3 skyDown)
{
    float up = direction.y;
    return up >= 0.0 ? lerp(skyHorizon, skyUp, up) : lerp(skyHorizon, skyDown, -up);
}

// ---------------------------------------------------------------- The radiance cache

// The radiance cache: a grid of probes round the camera, each holding the
// light that reaches its middle from every direction, as cone tracing found
// it (gi_inject.slang), so each pixel reads its eight nearest probes rather
// than tracing cones itself. A probe is two texels: the light's average
// colour and how much of the probe is open space; and the direction in
// which the light grows brighter, as the light's L1 spherical harmonic,
// relative to its average. Probes lie slice after slice across the image,
// the two texels' rows one above the other.

static const float GI_PROBE_WIDTH = float(GI_PROBES * GI_PROBES);

int2 giProbeTexel(int3 probe, int part)
{
    return int2(probe.x + probe.z * GI_PROBES, probe.y + part * GI_PROBES);
}

// What a probe sends towards a surface facing `normal`: its light over the
// half of the sphere the normal faces, weighted as light falls on a surface.
float3 giProbeLight(float4 average, float4 growth, float3 normal)
{
    return average.rgb * max(1.0 + 2.0 * dot(growth.xyz, normal), 0.0);
}

// The light falling on a surface from all round its normal, from the eight
// probes round it, each counted less the more it lies behind the surface
// and the less of it is open space, so light leaks less through walls.
// Beyond the grid, `fallback`, the plain sky light, fading in over its edge.
float3 giAmbient(Frame* frame, float3 position, float3 normal, float3 fallback)
{
    float spacing = frame->gi.probeSpacing;
    float3 inGrid = (position + normal * spacing * 0.5) / spacing - float3(frame->gi.probePreviousOrigin.xyz) - 0.5;
    float last = float(GI_PROBES - 1);
    float edge = min(min(inGrid.x, inGrid.y), min(inGrid.z, min(min(last - inGrid.x, last - inGrid.y), last - inGrid.z)));
    if (edge <= 0.0)
    {
        return fallback;
    }
    int3 base = int3(floor(inGrid));
    float3 within = inGrid - float3(base);
    Texture2D probes = textures[NonUniformResourceIndex(frame->gi.probeTexture)];
    float3 total = 0.0;
    float weights = 0.0;
    for (uint corner = 0; corner < 8; corner++)
    {
        int3 offset = int3(corner & 1, (corner >> 1) & 1, (corner >> 2) & 1);
        int3 probe = min(base + offset, GI_PROBES - 1);
        float3 trilinear = lerp(1.0 - within, within, float3(offset));
        float3 middle = (float3(probe + frame->gi.probePreviousOrigin.xyz) + 0.5) * spacing;
        float3 towards = normalize(middle - position + 1e-4);
        float facing = (dot(towards, normal) + 1.0) * 0.5;
        float4 average = probes.Load(int3(giProbeTexel(probe, 0), 0));
        float4 growth = probes.Load(int3(giProbeTexel(probe, 1), 0));
        float weight = trilinear.x * trilinear.y * trilinear.z * (facing * facing + 0.2) * max(average.a, 0.05);
        total += giProbeLight(average, growth, normal) * weight;
        weights += weight;
    }
    float3 cached = total / max(weights, 1e-5);
    return lerp(fallback, cached, saturate(edge / 2.0));
}
