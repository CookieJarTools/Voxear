// Ray-traced GI's probes as shading reads them (ddgi.slang fills them):
// a grid of probes round the camera in each cascade, each holding, in
// small octahedral maps, the light reaching it from every direction and how
// far its rays went before they hit something. A surface takes light from
// the eight probes round it, each only as far as its rays say it can see
// the surface, so a probe on the far side of a wall gives none: light does
// not pass walls.
//
// Probes are kept in a grid that scrolls with the camera, each world probe
// in a slot by its coordinates modulo the grid's size, so a probe keeps its
// slot while the camera moves. Each probe's maps are a tile of the atlases,
// 128 tiles across, a band of rows for each cascade.

#pragma once

#include "common.slang"

static const uint DDGI_TILES_ACROSS = 128;
// The least total weight a point's probes are divided by: below it, their light fades rather than being
// stretched to full strength from probes that can barely see the point.
static const float DDGI_MIN_WEIGHT = 0.05;
static const uint DDGI_PROBE_COUNT = uint(DDGI_PROBES.x * DDGI_PROBES.y * DDGI_PROBES.z);
static const uint DDGI_TILE_ROWS = DDGI_PROBE_COUNT / DDGI_TILES_ACROSS;

float ddgiSpacing(Frame* frame, uint cascade)
{
    return frame->ddgi.spacing * (cascade == 0 ? 1.0 : DDGI_CASCADE_SCALE);
}

int3 ddgiSlot(int3 world)
{
    return ((world % DDGI_PROBES) + DDGI_PROBES) % DDGI_PROBES;
}

uint ddgiIndex(int3 slot)
{
    return uint(slot.x + DDGI_PROBES.x * (slot.z + DDGI_PROBES.z * slot.y));
}

int3 ddgiSlotOf(uint index)
{
    int i = int(index);
    return int3(i % DDGI_PROBES.x, i / (DDGI_PROBES.x * DDGI_PROBES.z), (i / DDGI_PROBES.x) % DDGI_PROBES.z);
}

// The world probe a slot holds, given the cascade's lowest probe.
int3 ddgiWorldOf(int3 slot, int3 origin)
{
    return origin + ((slot - origin) % DDGI_PROBES + DDGI_PROBES) % DDGI_PROBES;
}

int2 ddgiTile(uint index, uint cascade)
{
    return int2(index % DDGI_TILES_ACROSS, index / DDGI_TILES_ACROSS + cascade * DDGI_TILE_ROWS);
}

// Where a probe is: the middle of its cell, moved by however far it has
// been moved off the surfaces round it (its state's xyz, in spacings).
float3 ddgiProbePosition(Frame* frame, uint cascade, int3 world)
{
    float spacing = ddgiSpacing(frame, cascade);
    int2 tile = ddgiTile(ddgiIndex(ddgiSlot(world)), cascade);
    float3 moved = textures[NonUniformResourceIndex(frame->ddgi.probes)].Load(int3(tile, 0)).xyz;
    return (float3(world) + 0.5 + moved) * spacing;
}

float2 octEncode(float3 n)
{
    n /= abs(n.x) + abs(n.y) + abs(n.z);
    float2 folded = (1.0 - abs(n.zx)) * select(n.xz >= 0.0, 1.0, -1.0);
    return n.y >= 0.0 ? n.xz : folded;
}

float3 octDecode(float2 e)
{
    float3 n = float3(e.x, 1.0 - abs(e.x) - abs(e.y), e.y);
    if (n.y < 0.0)
    {
        n.xz = (1.0 - abs(n.zx)) * select(n.xz >= 0.0, 1.0, -1.0);
    }
    return normalize(n);
}

// A probe's map in `atlas`, towards `direction`, filtered within its tile.
float4 ddgiMap(uint atlas, uint2 size, int2 tile, float3 direction)
{
    float2 inTile = clamp((octEncode(direction) * 0.5 + 0.5) * float(DDGI_TEXELS), 0.5, float(DDGI_TEXELS) - 0.5);
    float2 uv = (float2(tile) * float(DDGI_TEXELS) + inTile) / float2(size);
    return textures[NonUniformResourceIndex(atlas)].SampleLevel(samplers[SAMPLER_LINEAR_CLAMP], uv, 0.0);
}

// The light falling on a surface facing `normal` at `position`, looked up
// from a little way off it along `away`, from one cascade's probes; beyond
// the cascade, `fallback`, fading in over its edge.
float3 ddgiIrradianceIn(Frame* frame, uint cascade, float3 position, float3 normal, float3 away, float3 fallback)
{
    float spacing = ddgiSpacing(frame, cascade);
    float3 biased = position + away * spacing * 0.45;
    float3 inGrid = biased / spacing - float3(frame->ddgi.origin[cascade].xyz) - 0.5;
    float3 last = float3(DDGI_PROBES - 1);
    float3 inside = min(inGrid, last - inGrid);
    float edge = min(inside.x, min(inside.y, inside.z));
    if (edge <= 0.0)
    {
        return fallback;
    }
    int3 base = int3(floor(inGrid));
    float3 within = inGrid - float3(base);
    uint2 irradianceSize = uint2(DDGI_TILES_ACROSS * DDGI_TEXELS, DDGI_TILE_ROWS * DDGI_CASCADES * DDGI_TEXELS);
    Texture2D states = textures[NonUniformResourceIndex(frame->ddgi.probes)];
    float3 total = 0.0;
    float weights = 0.0;
    for (uint corner = 0; corner < 8; corner++)
    {
        int3 offset = int3(corner & 1, (corner >> 1) & 1, (corner >> 2) & 1);
        int3 world = min(base + offset, DDGI_PROBES - 1) + frame->ddgi.origin[cascade].xyz;
        uint index = ddgiIndex(ddgiSlot(world));
        int2 tile = ddgiTile(index, cascade);
        // Probes inside walls see only the wall's inside, and give nothing.
        if (states.Load(int3(tile, 0)).w < 0.5)
        {
            continue;
        }
        float3 probe = ddgiProbePosition(frame, cascade, world);
        float3 towards = normalize(probe - position);
        float facing = (dot(towards, normal) + 1.0) * 0.5;
        float weight = facing * facing + 0.2;
        // How far the probe's rays went towards this point: beyond that, a wall is in the way (Chebyshev).
        float3 away = biased - probe;
        float distance = length(away);
        float2 moments = ddgiMap(frame->ddgi.depth, irradianceSize, tile, away / max(distance, 1e-4)).xy;
        float variance = abs(moments.x * moments.x - moments.y);
        float seen = distance > moments.x ? variance / (variance + (distance - moments.x) * (distance - moments.x)) : 1.0;
        // Barely at all where its rays say a wall is between: a probe outside sees a sky many times brighter
        // than a room, so even a small share would show.
        weight *= max(1e-4, seen * seen * seen);
        weight = max(weight, 1e-6);
        // Faint weights fall off faster still, so a barely seen probe barely counts.
        if (weight < 0.2)
        {
            weight *= weight * weight / 0.04;
        }
        float3 trilinear = lerp(1.0 - within, within, float3(offset));
        weight *= trilinear.x * trilinear.y * trilinear.z;
        total += ddgiMap(frame->ddgi.irradiance, irradianceSize, tile, normal).rgb * weight;
        weights += weight;
    }
    // Where the probes round a point can hardly see it, their light is not stretched to fill in: a point no probe
    // sees is dark, not lit by the sky or by probes beyond a wall.
    float3 cached = total / max(weights, DDGI_MIN_WEIGHT);
    return lerp(fallback, cached, saturate(edge / 2.0));
}

// The light round a point from every direction, for fog to be lit by: the
// light falling from above and from below, averaged. The point is taken a
// little towards the camera, `toCamera`, which keeps a point on a ceiling
// or a wall on the side the camera sees rather than pushing it through.
float3 ddgiLightAround(Frame* frame, float3 position, float3 toCamera, float3 fallback)
{
    float3 up = float3(0.0, 1.0, 0.0);
    float3 above = ddgiIrradianceAlong(frame, position, up, toCamera, fallback);
    float3 below = ddgiIrradianceAlong(frame, position, -up, toCamera, fallback);
    return (above + below) * 0.5;
}

// The light falling on a surface from all round its normal, from the fine
// cascade near the camera, the coarse one past it, and beyond both
// `fallback`, the plain sky light, looked up from a little way along `away`.
float3 ddgiIrradianceAlong(Frame* frame, float3 position, float3 normal, float3 away, float3 fallback)
{
    float3 far = ddgiIrradianceIn(frame, 1, position, normal, away, fallback);
    return ddgiIrradianceIn(frame, 0, position, normal, away, far) * frame->ddgi.strength;
}

// As ddgiIrradianceAlong, for a surface seen from `view` (towards the
// camera): looked up off the surface, towards the viewer and along the
// normal, so the surface does not shadow itself, and so a point in a corner
// is looked up from inside the room rather than from the seam, where probes
// beyond the walls can nearly see it.
float3 ddgiIrradiance(Frame* frame, float3 position, float3 normal, float3 view, float3 fallback)
{
    return ddgiIrradianceAlong(frame, position, normal, normal * 0.4 + view * 0.6, fallback);
}
