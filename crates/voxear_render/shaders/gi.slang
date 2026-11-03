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
// last frame left them; nothing outside the grid. Their light, weighted by
// how much of them is filled, and how much is.
float4 giSample(Frame* frame, float3 position, uint level)
{
    return giSampleAtlas(frame, frame->gi.texture, position, level);
}

// As giSample, from the atlas `atlas` laid out as the voxels are.
float4 giSampleAtlas(Frame* frame, uint atlas, float3 position, uint level)
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
    Texture2D voxels = textures[NonUniformResourceIndex(atlas)];
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
        // The grid whose voxels are as wide as the cone, so each step, as long as the cone is wide, misses none.
        float level = min(ceil(log2(width / smallest)), coarsest);
        float3 at = start + direction * distance;
        float4 voxel = giSample(frame, at, uint(level));
        // A step as long as the cone is wide, so each sample covers fresh space.
        distance += width;
        // Most of a cone's way is through empty air, where nothing more need be looked up.
        if (voxel.a < 1e-3)
        {
            continue;
        }
        // How much the voxels block along the cone: a floor in a coarse voxel blocks a cone going down through
        // it, but little of one passing level above it.
        float3 axes = giSampleAtlas(frame, frame->gi.opacityTexture, at, uint(level)).xyz;
        float3 squared = direction * direction;
        float blocks = saturate(dot(axes, squared));
        // What fills them, as bright as it is, over as much as blocks, and only as far as its light faces back
        // along the cone: the dark side of a wall shows none of its sunlit side's light, though both share a voxel.
        float3 lightward = giSampleAtlas(frame, frame->gi.directionTexture, at, uint(level)).xyz;
        float toward = length(lightward) > 1e-4 ? saturate(0.5 - 1.5 * dot(direction, normalize(lightward))) : 1.0;
        light += (1.0 - blocked) * voxel.rgb / max(voxel.a, 1e-3) * blocks * toward;
        blocked += (1.0 - blocked) * blocks;
    }
    return float4(light, blocked);
}

// What a surface reflects of the voxels in a direction, in a cone as wide
// as its roughness spreads the reflection: their light, and how much of the
// cone they block, so the sky shows through the rest.
float4 giReflection(Frame* frame, float3 position, float3 normal, float3 direction, float roughness)
{
    float tanHalfAngle = clamp(roughness * roughness * 1.5, 0.05, 1.0);
    float4 cone = giCone(frame, position + normal * frame->gi.voxelSize, direction, tanHalfAngle);
    return float4(cone.rgb * frame->gi.strength, cone.a);
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
// than tracing cones itself. A probe is an ambient cube: six texels, the
// light falling on a surface facing +X, -X, +Y, -Y, +Z and -Z, each with
// how much of the probe is open space. A surface takes the faces it turns
// towards, so a floor is lit by what lies above it rather than by its own
// light coming back. Probes lie slice after slice across the image, the six
// texels' rows one above the other.

static const float GI_PROBE_WIDTH = float(GI_PROBES * GI_PROBES);
static const int GI_PROBE_FACES = 6;

// A probe's texel for `part`, its cube's face, in `cascade`'s band of rows.
int2 giProbeTexel(int3 probe, int part, uint cascade)
{
    return int2(probe.x + probe.z * GI_PROBES, probe.y + (part + int(cascade) * GI_PROBE_FACES) * GI_PROBES);
}

// How far apart a cascade's probes are, in metres.
float giProbeSpacing(Frame* frame, uint cascade)
{
    return frame->gi.probeSpacing * (cascade == 0 ? 1.0 : GI_CASCADE_SCALE);
}

// How far inside a cascade's grid a point is, in probes from its nearest
// edge, and where it lies in the grid.
float giInCascade(Frame* frame, uint cascade, float3 position, out float3 inGrid)
{
    inGrid = position / giProbeSpacing(frame, cascade) - float3(frame->gi.probePreviousOrigin[cascade].xyz) - 0.5;
    float last = float(GI_PROBES - 1);
    return min(min(inGrid.x, inGrid.y), min(inGrid.z, min(min(last - inGrid.x, last - inGrid.y), last - inGrid.z)));
}

// What a probe sends towards a surface facing `normal`: the faces of its
// cube the normal turns towards, each as much as it turns that way, and
// how open the probe is.
float4 giProbeLight(Texture2D probes, int3 probe, uint cascade, float3 normal)
{
    float3 squared = normal * normal;
    float4 x = probes.Load(int3(giProbeTexel(probe, normal.x >= 0.0 ? 0 : 1, cascade), 0));
    float4 y = probes.Load(int3(giProbeTexel(probe, normal.y >= 0.0 ? 2 : 3, cascade), 0));
    float4 z = probes.Load(int3(giProbeTexel(probe, normal.z >= 0.0 ? 4 : 5, cascade), 0));
    return float4(x.rgb * squared.x + y.rgb * squared.y + z.rgb * squared.z, x.a);
}

// A probe's light from all round, the average of its six faces.
float3 giProbeAverage(Texture2D probes, int3 probe, uint cascade)
{
    float3 total = 0.0;
    for (int face = 0; face < GI_PROBE_FACES; face++)
    {
        total += probes.Load(int3(giProbeTexel(probe, face, cascade), 0)).rgb;
    }
    return total / float(GI_PROBE_FACES);
}

// The light round a point from every direction, as one cascade's probes
// round it hold it; beyond the cascade, `fallback`, fading in over its edge.
float3 giLightAroundIn(Frame* frame, uint cascade, float3 position, float3 fallback)
{
    float3 inGrid;
    float edge = giInCascade(frame, cascade, position, inGrid);
    if (edge <= 0.0)
    {
        return fallback;
    }
    int3 base = int3(floor(inGrid));
    float3 within = inGrid - float3(base);
    Texture2D probes = textures[NonUniformResourceIndex(frame->gi.probeTexture)];
    float3 total = 0.0;
    for (uint corner = 0; corner < 8; corner++)
    {
        int3 offset = int3(corner & 1, (corner >> 1) & 1, (corner >> 2) & 1);
        int3 probe = min(base + offset, GI_PROBES - 1);
        float3 trilinear = lerp(1.0 - within, within, float3(offset));
        total += giProbeAverage(probes, probe, cascade) * trilinear.x * trilinear.y * trilinear.z;
    }
    return lerp(fallback, total, saturate(edge / 2.0));
}

// The light round a point from every direction, for fog to be lit by: dim
// indoors, where walls keep the sky out. The fine cascade near the camera,
// the coarse one past it, and beyond both `fallback`, the plain sky light.
float3 giLightAround(Frame* frame, float3 position, float3 fallback)
{
    if (frame->gi.probeTexture == NO_TEXTURE)
    {
        return fallback;
    }
    float3 far = giLightAroundIn(frame, 1, position, fallback);
    return giLightAroundIn(frame, 0, position, far);
}

// How much of the open sky's light reaches a point, from 0 deep inside to
// 1 out in the open, for fog lit by a single colour.
float giOpenness(Frame* frame, float3 position, float3 openSky)
{
    static const float3 LUMINANCE = float3(0.2126, 0.7152, 0.0722);
    float sky = max(dot(openSky, LUMINANCE), 1e-5);
    return saturate(dot(giLightAround(frame, position, openSky), LUMINANCE) / sky);
}

// The light falling on a surface from all round its normal, from the eight
// probes of a cascade round it, each counted less the more it lies behind
// the surface and the less of it is open space, and hardly at all with a
// wall between, so light leaks less through walls. Beyond the cascade,
// `fallback`, fading in over its edge.
float3 giAmbientIn(Frame* frame, uint cascade, float3 position, float3 normal, float3 fallback)
{
    float spacing = giProbeSpacing(frame, cascade);
    float3 inGrid;
    float edge = giInCascade(frame, cascade, position + normal * spacing * 0.5, inGrid);
    if (edge <= 0.0)
    {
        return fallback;
    }
    // The voxels a wall between is looked for in: as coarse as the probes are far apart.
    uint level = cascade == 0 ? 0 : GI_LEVELS - 1;
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
        float3 middle = (float3(probe + frame->gi.probePreviousOrigin[cascade].xyz) + 0.5) * spacing;
        float3 towards = normalize(middle - position + 1e-4);
        float facing = (dot(towards, normal) + 1.0) * 0.5;
        float4 light = giProbeLight(probes, probe, cascade, normal);
        // A wall between the surface and the probe, found in the voxels halfway to it, hides most of its light.
        float between = giSample(frame, lerp(position + normal * spacing * 0.25, middle, 0.5), level).a;
        float visible = 1.0 - 0.9 * saturate(between);
        float weight = trilinear.x * trilinear.y * trilinear.z * (facing * facing + 0.2) * max(light.a, 0.05) * visible;
        total += light.rgb * weight;
        weights += weight;
    }
    float3 cached = total / max(weights, 1e-5);
    return lerp(fallback, cached, saturate(edge / 2.0));
}

// The light falling on a surface from all round its normal: from the fine
// cascade of probes near the camera, the coarse one past it, and beyond
// both `fallback`, the plain sky light.
float3 giAmbient(Frame* frame, float3 position, float3 normal, float3 fallback)
{
    float3 far = giAmbientIn(frame, 1, position, normal, fallback);
    return giAmbientIn(frame, 0, position, normal, far);
}
