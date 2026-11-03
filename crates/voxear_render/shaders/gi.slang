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

// The light falling on a surface from all round its normal: five cones, the
// one along the normal counting most, each adding the voxels' bounced light
// and, where they leave it open, the sky's.
float3 giAmbient(Frame* frame, float3 position, float3 normal, float3 skyUp, float3 skyHorizon, float3 skyDown)
{
    float3 side = abs(normal.y) < 0.9 ? float3(0.0, 1.0, 0.0) : float3(1.0, 0.0, 0.0);
    float3 tangent = normalize(cross(side, normal));
    float3 bitangent = cross(normal, tangent);
    float3 start = position + normal * frame->gi.voxelSize;
    // 60 degree cones: one along the normal, four leaning 45 degrees from it.
    static const float TAN_HALF = 0.577;
    float3 directions[5] = {
        normal,
        normalize(normal + tangent),
        normalize(normal - tangent),
        normalize(normal + bitangent),
        normalize(normal - bitangent),
    };
    float weights[5] = { 0.4, 0.15, 0.15, 0.15, 0.15 };
    uint cones = clamp(frame->gi.cones, 1, 5);
    float3 total = 0.0;
    float weighting = 0.0;
    for (uint i = 0; i < cones; i++)
    {
        float4 cone = giCone(frame, start, directions[i], TAN_HALF);
        float up = directions[i].y;
        float3 sky = up >= 0.0 ? lerp(skyHorizon, skyUp, up) : lerp(skyHorizon, skyDown, -up);
        total += weights[i] * (cone.rgb * frame->gi.strength + (1.0 - cone.a) * sky);
        weighting += weights[i];
    }
    return total / weighting;
}
