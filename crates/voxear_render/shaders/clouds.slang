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

// What the weather over parts of the level adds to the clouds over `xz` in the world: how far it fills them in
// towards overcast, and how much heavier it makes them, each from 0 to 1.
float2 cloudWeather(Frame* frame, float2 xz)
{
    float2 adds = 0.0;
    for (uint i = 0; i < frame->sea.storms; i++)
    {
        adds = max(adds, frame->sea.skies[i].xy * weatherShare(frame, i, xz));
    }
    return adds;
}

// What falls from the weather over parts of the level, seen from outside it as a curtain hanging under its cloud,
// uneven, and thinner towards the ground; blown sand is a wall a few hundred metres high. Gives how much of what
// lies `distance` metres off along `direction` shows through all of it, in `sand` how much of what hides the rest
// is blown sand rather than rain or snow, and in `dark` how dark the weather it falls from is.
static const float SAND_WALL = 350.0;
float weatherCurtain(Frame* frame, float3 direction, float distance, out float sand, out float dark)
{
    sand = 0.0;
    dark = 0.0;
    float flat = length(direction.xz);
    if (frame->sea.storms == 0 || flat < 1e-4)
    {
        return 1.0;
    }
    float2 along = direction.xz / flat;
    float reach = distance * flat;
    Clouds clouds = frame->lighting.clouds;
    // How high the camera is over the level's ground, and how high the clouds' undersides are.
    float camera = frame->cameraPosition.y + frame->origin.w;
    float under = clouds.base + 0.15 * clouds.thickness;
    float through = 1.0;
    float hidden = 0.0;
    for (uint i = 0; i < frame->sea.storms; i++)
    {
        float4 place = frame->sea.places[i];
        float4 sky = frame->sea.skies[i];
        float2 falls = sky.zw * float2(1.0, 2.5);
        if (falls.x + falls.y <= 0.0)
        {
            continue;
        }
        float2 to = place.xy - frame->cameraPosition.xz;
        float nearest = dot(to, along);
        float aside = dot(to, to) - nearest * nearest;
        float within = place.z * place.z - aside;
        if (within <= 0.0)
        {
            continue;
        }
        float half = sqrt(within);
        float from = max(nearest - half, 0.0);
        float until = min(nearest + half, reach);
        if (until <= from)
        {
            continue;
        }
        const uint SAMPLES = 6;
        float2 thick = 0.0;
        for (uint j = 0; j < SAMPLES; j++)
        {
            float t = lerp(from, until, (float(j) + 0.5) / float(SAMPLES));
            float2 xz = frame->cameraPosition.xz + along * t;
            float high = camera + direction.y * t / flat;
            // Heavier here and lighter there, in shafts.
            float shafts = 0.45 + 1.1 * cloudPatches(levelXZ(frame, xz, 65536.0) / max(place.z * 0.22, 40.0));
            float share = weatherShare(frame, i, xz) * shafts;
            // Rain and snow from the clouds' undersides down, thinning as they fall; sand from the ground up.
            float rain = saturate((under - high) / (0.1 * clouds.thickness)) * (0.5 + 0.5 * saturate(high / under));
            float wall = saturate((camera + SAND_WALL * (0.7 + 0.6 * shafts) - high) / 60.0);
            thick += share * float2(rain, wall) * falls;
        }
        // A heavy fall hides what is a few hundred metres into it.
        thick *= (until - from) / float(SAMPLES) / flat * 0.004;
        float kept = exp(-thick.x - thick.y);
        float added = through * (1.0 - kept);
        sand += added * thick.y / max(thick.x + thick.y, 1e-6);
        dark += added * sky.y;
        hidden += added;
        through *= kept;
    }
    sand = hidden > 0.0 ? sand / hidden : 0.0;
    dark = hidden > 0.0 ? dark / hidden : 0.0;
    return through;
}

// The colour of what falls from weather far off, against a sky as bright as `lit`: grey, the darker the heavier
// the weather, or the colour of blown sand.
float3 curtainColour(float3 lit, float sand, float dark)
{
    float bright = dot(lit, float3(0.2126, 0.7152, 0.0722));
    return lerp(bright * (0.75 - 0.4 * dark), float3(0.56, 0.4, 0.22) * bright * 1.7, sand);
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
    // Weather over this part of the level fills them in; a sky with no clouds of its own has them only there.
    float filled = cloudWeather(frame, at + clouds.drift).x;
    if (clouds.cover <= 0.001 && filled <= 0.0)
    {
        return -1.0;
    }
    cover += (1.0 - cover) * filled;
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
        float cube = cloudCubeAt(frame, at, height);
        return cube > 0.0 ? cube * clouds.density * CLOUD_THICK * (1.0 + cloudWeather(frame, at + clouds.drift).y) : 0.0;
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
    // Thick soon after the edge, and heavier under weather.
    return saturate(inside * 6.0) * clouds.density * CLOUD_THICK * (1.0 + cloudWeather(frame, at + clouds.drift).y);
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
