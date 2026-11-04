// Impostors: a plant far from the camera drawn as a picture of it on a
// quad, from an atlas of pictures taken from all round its upper half
// (src/impostors.rs bakes them). The quad turns to the picture taken from
// nearest the camera's side and shows that, its normals from the picture
// too, so it is lit as the plant would be. Included by shading.slang.

#pragma once

#include "common.slang"

// Pictures across and down an atlas, as IMPOSTOR_VIEWS in src/impostors.rs.
static const uint IMPOSTOR_VIEWS = 8;

bool isImpostor(Instance instance)
{
    return (instance.flags & INSTANCE_IMPOSTOR) != 0;
}

// The direction picture `cell` looks at the plant from, in the plant's own
// space: the hemi-octahedral mapping of picture_direction in src/impostors.rs.
float3 pictureDirection(uint2 cell)
{
    float2 g = (float2(cell) + 0.5) / float(IMPOSTOR_VIEWS) * 2.0 - 1.0;
    float2 p = float2(g.x + g.y, g.x - g.y) * 0.5;
    return normalize(float3(p.x, 1.0 - abs(p.x) - abs(p.y), p.y));
}

// The picture taken from nearest `toward`, in the plant's own space; from
// below, the one from level with it.
uint2 nearestPicture(float3 toward)
{
    toward.y = max(toward.y, 0.0);
    toward /= max(abs(toward.x) + abs(toward.y) + abs(toward.z), 1e-6);
    float2 g = float2(toward.x + toward.z, toward.x - toward.z);
    return uint2(clamp(floor((g * 0.5 + 0.5) * float(IMPOSTOR_VIEWS)), 0.0, float(IMPOSTOR_VIEWS - 1)));
}

struct ImpostorCorner
{
    float3 world;
    float3 facing;
    float2 uv;
};

// One corner of an impostor's quad, `corner` from -1 to 1 across and up:
// where it is in the world, the way the picture faces there, and where the
// corner falls in the atlas. The instance's model places the sphere the
// plant was pictured in, turned as the plant is and scaled to its radius.
ImpostorCorner impostorCorner(Instance instance, float3 camera, float2 corner)
{
    float3 centre = instance.model.c3.xyz;
    float3 toCamera = camera - centre;
    float3 local = normalizeOr(float3(dot(instance.model.c0.xyz, toCamera), dot(instance.model.c1.xyz, toCamera), dot(instance.model.c2.xyz, toCamera)), float3(0.0, 1.0, 0.0));
    uint2 cell = nearestPicture(local);
    float3 toward = pictureDirection(cell);
    // The picture's own camera axes, as glam's left-handed look_at makes them.
    float3 up = abs(toward.y) > 0.99 ? float3(0.0, 0.0, 1.0) : float3(0.0, 1.0, 0.0);
    float3 across = normalize(cross(toward, up));
    float3 upward = cross(across, toward);

    ImpostorCorner o;
    o.world = transform(instance.model, float4(across * corner.x + upward * corner.y, 1.0)).xyz;
    o.facing = normalize(transform(instance.model, float4(toward, 0.0)).xyz);
    // The pictures are drawn with Y flipped, as the cameras are, so the top of the plant is the top of its cell.
    o.uv = (float2(cell) + float2(corner.x, -corner.y) * 0.5 + 0.5) / float(IMPOSTOR_VIEWS);
    return o;
}

// An impostor's normal at `uv` of its atlas, in the world.
float3 impostorNormal(Instance instance, Material material, float2 uv, float3 otherwise)
{
    float3 local = sampleTexture(material.normalTexture, uv).xyz * 2.0 - 1.0;
    return normalizeOr(transform(instance.model, float4(local, 0.0)).xyz, otherwise);
}
