// The sky's clouds (src/clouds.rs): a layer between two heights over the level's ground, shaped from the baked
// tiling noise the smoke is shaped from (smoke::noise_atlas), its round puffs their bodies and its swirls their
// wisps. Here is what everything reads of them: how thick the cloud is at a place, and how much of the sun gets
// through to a point under them, from the map the Cloud shadows pass draws. The passes that draw them are in
// sky.slang.

#pragma once

#include "common.slang"

// How much light a metre of the thickest cloud takes out, for a density of 1: a fair-weather cloud is opaque
// within a few tens of metres.
static const float CLOUD_THICK = 0.02;
// The baked noise's layout: 64 texels a side, each slice of depth a tile 66 wide of an atlas 528 across, with a
// period of 8.
static const float CLOUD_NOISE_SIDE = 64.0;
static const float CLOUD_NOISE_TILE = 66.0;
static const float CLOUD_NOISE_ATLAS = 528.0;
static const float CLOUD_NOISE_CELLS = 8.0;

// Clouds::kind.
static const uint CLOUDS_SOFT = 0;
static const uint CLOUDS_BLOCKS = 1;

// Whether the frame has clouds.
bool cloudsOn(Frame* frame)
{
    return frame->lighting.clouds.cover > 0.0 && frame->lighting.clouds.noise != NO_TEXTURE;
}

// The baked noise at `p`, period 8: swirls in red, green and blue, round puffs in alpha, each about a half.
float4 cloudNoise(Frame* frame, float3 p)
{
    Texture2D atlas = textures[NonUniformResourceIndex(frame->lighting.clouds.noise)];
    float3 t = frac(p / CLOUD_NOISE_CELLS) * CLOUD_NOISE_SIDE;
    float slice = floor(t.z);
    float next = fmod(slice + 1.0, CLOUD_NOISE_SIDE);
    let read = (float s) => atlas.SampleLevel(samplers[SAMPLER_LINEAR_CLAMP],
        (float2(fmod(s, 8.0), floor(s / 8.0)) * CLOUD_NOISE_TILE + 1.0 + t.xy + 0.5) / CLOUD_NOISE_ATLAS, 0.0);
    return lerp(read(slice), read(next), t.z - slice);
}

// A number from 0 to 1 for a cell of the plane, from whole numbers alone.
float cloudCell(int2 cell)
{
    uint h = uint(cell.x) * 0x8DA6B343u ^ uint(cell.y) * 0xD8163841u;
    h ^= h >> 16;
    h *= 0x7FEB352Du;
    h ^= h >> 15;
    h *= 0x846CA68Bu;
    h ^= h >> 16;
    return float(h >> 8) / 16777216.0;
}

// Smooth noise over the plane, from 0 to 1: where the sky has more cloud and where less.
float cloudPatches(float2 p)
{
    float2 base = floor(p);
    int2 cell = int2(base);
    float2 f = p - base;
    f = f * f * (3.0 - 2.0 * f);
    float a = cloudCell(cell);
    float b = cloudCell(cell + int2(1, 0));
    float c = cloudCell(cell + int2(0, 1));
    float d = cloudCell(cell + int2(1, 1));
    return lerp(lerp(a, b, f.x), lerp(c, d, f.x), f.y);
}

// How far inside a cloud `at` is in the clouds' own pattern, which drifts over the world, `height` of the way up the
// layer: 0 at a cloud's edge, 1 well inside, less than 0 outside. `q` is the place in the noise.
float cloudInside(Frame* frame, float2 at, float height, out float3 q)
{
    Clouds clouds = frame->lighting.clouds;
    // Below a quarter of the way up, a cloud keeps the outline it has there: the big ones all stand on the same flat
    // underside, as clouds that formed at one height do.
    q = float3(at.x, max(height, 0.25) * clouds.thickness, at.y) / clouds.size;
    // Big round puffs with smaller ones on them.
    float body = cloudNoise(frame, q).a * 0.7 + cloudNoise(frame, q * 2.3 + 3.1).a * 0.3;
    // More cloud here and less there, over several clouds' width; none of that under a clear or an overcast sky.
    float patches = cloudPatches(at / (clouds.size * 5.0));
    float cover = saturate(clouds.cover + (patches - 0.5) * 0.6 * (1.0 - abs(clouds.cover * 2.0 - 1.0)));
    // Flat undersides and rounded tops: the higher, the less of each puff is cloud.
    float threshold = lerp(0.82, 0.22, cover) + 0.25 * height * height;
    return (body - threshold) / 0.25;
}

// How much of a cube of a cloud built of them is cloud, from 0 for none to 1, for the cube `at` and `height` are in:
// every place in a cube is as the cube's middle is. A solid cube is all cloud or none; a soft one is the fainter the
// nearer its cloud's edge.
float cloudCubeAt(Frame* frame, float2 at, float height)
{
    Clouds clouds = frame->lighting.clouds;
    at = (floor(at / clouds.block) + 0.5) * clouds.block;
    height = (floor(height * clouds.thickness / clouds.block) + 0.5) * clouds.block / clouds.thickness;
    if (height <= 0.0 || height >= 1.0)
    {
        return 0.0;
    }
    float3 q;
    // Soft clouds lose their thin edges, so as much is left out here.
    float inside = cloudInside(frame, at, height, q) - 0.2;
    if (inside <= 0.0)
    {
        return 0.0;
    }
    return clouds.softness > 0.0 ? saturate(0.15 + inside / (0.8 * clouds.softness)) : 1.0;
}

// How much light a metre of cloud takes out at `at` in the clouds' own pattern, `height` of the way up the layer
// from its undersides (0) to its tops (1): nothing outside clouds. `fine` adds the bites out of their edges, which
// their shadows and the light through them do without.
float cloudDensityIn(Frame* frame, float2 at, float height, bool fine)
{
    Clouds clouds = frame->lighting.clouds;
    if (clouds.kind == CLOUDS_BLOCKS)
    {
        return cloudCubeAt(frame, at, height) * clouds.density * CLOUD_THICK;
    }
    if (height <= 0.0 || height >= 1.0)
    {
        return 0.0;
    }
    float3 q;
    // Drawn in a little the lower it goes under a quarter of the way up, so wisps too thin to reach the underside
    // hang above it.
    float inside = saturate(cloudInside(frame, at, height, q) - max(0.25 - height, 0.0) * 1.4);
    inside *= saturate(height / 0.06) * saturate((1.0 - height) / 0.15);
    if (inside <= 0.0)
    {
        return 0.0;
    }
    // Deeper in than the edge's bites reach, the cloud is whole.
    if (fine && inside < 0.65)
    {
        // Puffs a fifth the size and smaller ones again bitten out of the edge, which give it its cauliflower
        // outline.
        float puffs = cloudNoise(frame, q * 4.7 + 1.7).a;
        float small = cloudNoise(frame, q * 13.1 + 5.3).a;
        float bite = ((1.0 - puffs) * 0.6 + (1.0 - small) * 0.4) * 0.55;
        inside = saturate((inside - bite) / (1.0 - bite));
    }
    // Thick soon after the edge.
    return saturate(inside * 6.0) * clouds.density * CLOUD_THICK;
}

// As cloudDensityIn, at `xz` in the world.
float cloudDensityAt(Frame* frame, float2 xz, float height, bool fine)
{
    return cloudDensityIn(frame, xz - frame->lighting.clouds.drift, height, fine);
}

// How much of the sun's light the clouds let through to `position`, from 0 to 1, read from the frame's map of it
// along the way to the sun; all of it without clouds, over them, or with the sun at the horizon, where their
// shadows would run on without end.
float cloudShadowAt(Frame* frame, float3 position)
{
    Clouds clouds = frame->lighting.clouds;
    if (clouds.cover <= 0.0 || clouds.shadows <= 0.0 || clouds.shadowMap == NO_TEXTURE)
    {
        return 1.0;
    }
    float3 toSun = -frame->sunDirection;
    float below = clouds.base + clouds.thickness * 0.5 - levelHeight(frame, position);
    if (below <= 0.0 || toSun.y <= 0.02)
    {
        return 1.0;
    }
    // Where the way to the sun crosses the middle of the layer.
    float2 through = position.xz + toSun.xz / max(toSun.y, 0.12) * below;
    float2 uv = (through - clouds.shadowMiddle) / (2.0 * clouds.shadowReach) + 0.5;
    float lit = textures[NonUniformResourceIndex(clouds.shadowMap)].SampleLevel(samplers[SAMPLER_LINEAR_CLAMP], uv, 0.0).r;
    // Past the map's edge, as much as gets through on the whole.
    float2 out = abs(uv - 0.5) * 2.0;
    float inside = 1.0 - smoothstep(0.85, 1.0, max(out.x, out.y));
    lit = lerp(1.0 - 0.7 * clouds.cover, lit, inside);
    return lerp(1.0, lit, clouds.shadows * smoothstep(0.02, 0.12, toSun.y));
}
