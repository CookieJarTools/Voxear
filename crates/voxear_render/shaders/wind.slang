// Plants moving in the level's wind (see src/wind.rs), and pushed aside by
// what passes through them (src/foliage.rs): every pass that places a vertex
// moves it by windOffset, so a plant's shading, depth, shadow and motion all
// move together.
//
// A plant bends from the ground up, as far as its height above its
// instance's origin is up its whole height, so its pivot belongs at its
// base. It leans downwind as hard as the wind and the gust over it blow,
// sways back and forth and across the wind by the turbulence, and its
// leaves flutter along their own faces. Gusts are noise drifting downwind
// across the level, so neighbouring plants move together and a field shows
// waves going through it.
//
// A plant's vertices can say more (wind masks: see Vertex in src/mesh.rs,
// read from its vertex colours or worked out at import): how much each
// flutters, so a trunk stays still while its leaves move; a phase of its
// own, so neighbouring leaves and branches move out of step; and how far
// its branch sways apart from the trunk, bobbing as well as leaning.
//
// Under water there is no wind: a plant rooted under the sea sways back and
// forth with its swell instead, slowly, less the deeper it stands, a ripple
// running up it as the water's to and fro reaches its tip later than its
// root; under a Pool's still water it drifts a little.

#pragma once

#include "common.slang"

// How far the tip of a plant leans at strength 1, as a share of its height.
static const float WIND_LEAN = 0.08;
// How far leaves flutter at strength 1, in metres.
static const float WIND_FLUTTER = 0.02;
// How far a plant's tip leans away from a push of 1, as a share of its height.
static const float PUSH_LEAN = 0.8;
// How far a branch sways at strength 1, as a share of the plant's height.
static const float WIND_BRANCH = 0.05;

// How fast the air moves at strength 1, in metres a second.
static const float WIND_SPEED = 5.0;

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

// How hard the wind blows over `place`, across the ground from the level's
// middle (levelPosition), at `time`: its strength and the gust over it,
// noise drifting downwind, so it sweeps across the level. Plants lean by it
// and particles are carried by it, so a gust that bends the grass takes the
// smoke over it too.
float windPush(Wind wind, float2 place, float time)
{
    float2 drift = place / wind.gustSize - wind.direction * (time * wind.gustSpeed / wind.gustSize);
    float gust = windNoise(drift) * 0.7 + windNoise(drift * 2.3 + 17.0) * 0.3;
    return wind.strength * (1.0 + wind.gusts * (gust * 2.0 - 0.6));
}

// How fast the air moves over `place` at `time`, in metres a second, level
// and downwind: WIND_SPEED at strength 1, a breeze.
float3 windVelocity(Wind wind, float2 place, float time)
{
    if (wind.strength <= 0.0)
    {
        return 0.0;
    }
    return float3(wind.direction.x, 0.0, wind.direction.y) * (max(windPush(wind, place, time), 0.0) * WIND_SPEED);
}

// A plant's lean as its tip moves it: on an arc, lowering as it leans, so
// the plant keeps its length, and never further over than it is tall.
float3 bowed(float3 offset, float height)
{
    float across = length(offset.xz);
    if (across > height * 0.95)
    {
        offset.xz *= height * 0.95 / across;
    }
    offset.y -= dot(offset.xz, offset.xz) / (2.0 * height);
    return offset;
}

// How far under water the root of a plant standing at `origin` is, in
// metres, and whether that is the sea's; 0 out of it. Dry Areas are not
// asked: plants are seldom in them.
float plantUnderwater(Frame* frame, float3 origin, out bool sea)
{
    sea = false;
    for (uint i = 0; i < frame->ocean.pools; i++)
    {
        WaterBody pool = buffers()->waterBodies[i];
        float3 local = transform(pool.toLocal, float4(origin, 1.0)).xyz;
        if (pool.kind == WATER_POOL && pool.liquid == LIQUID_WATER && pool.shape == 0 && all(abs(local.xz) <= pool.size.xz * 0.5)
            && local.y < 0.0 && local.y > -pool.size.y)
        {
            return -local.y;
        }
    }
    sea = frame->ocean.enabled != 0 && origin.y < frame->ocean.level;
    if (!sea && origin.y < frame->sea.level)
    {
        // Under a world of blocks' sea.
        sea = true;
        return frame->sea.level - origin.y;
    }
    return sea ? frame->ocean.level - origin.y : 0.0;
}

// How far the water moves a plant `height` tall standing `below` metres
// under it, its vertex `up` of the way up it, at `time`: the sea's swell to
// and fro along its waves, with a little across, the tip later than the
// root; a Pool's water drifting it gently.
float3 surgeOffset(Frame* frame, float3 origin, float height, float below, bool sea, float up, float time)
{
    float3 along = float3(cos(frame->ocean.wind), 0.0, sin(frame->ocean.wind));
    float3 across = float3(-along.z, 0.0, along.x);
    // The swell passing over: each plant a little out of step with the next, as the wave reaches it. Where the plant
    // stands is read from the level's middle, so it keeps its step as the world's middle moves.
    float2 place = levelPosition(frame, origin, 4096.0).xz;
    float phase = dot(place, along.xz) * 0.12 + windHash(floor(place * 3.0)) * 0.8;
    float period = sea ? 5.0 + windHash(floor(place * 0.5)) : 4.0;
    float beat = time * 6.2831853 / period - phase - up * 1.2;
    // A world of blocks' sea moves its weed the more the rougher the weather over it makes it.
    float rough = frame->ocean.enabled != 0 ? saturate(frame->ocean.waveHeight * frame->ocean.waveScale / 3.0) : frame->sea.everywhere;
    for (uint i = 0; i < frame->sea.storms && frame->ocean.enabled == 0; i++)
    {
        rough = max(rough, frame->sea.roughs[i] * weatherShare(frame, i, origin.xz));
    }
    float strength = sea ? (0.12 + 0.25 * rough) * exp(-below / 12.0) : 0.06;
    float to = sin(beat) * 0.8 + sin(beat * 2.3 + 1.3) * 0.2;
    float side = sin(beat * 0.7 + phase * 2.1);
    return (along * to + across * side * 0.3) * strength * height * up * up;
}

// How tall a plant is: the top of its mesh's bounds above its origin, as its instance scales it.
float plantHeight(Instance instance, Mesh mesh)
{
    return max((mesh.center.y + mesh.radius) * length(instance.model.c1.xyz), 0.05);
}

// The push through foliage at a point, from the grid round the camera,
// blended between its four nearest cells; nothing beyond the grid.
float2 foliagePush(Frame* frame, float3 world)
{
    Pushes pushes = frame->pushes;
    float2 at = world.xz / pushes.cell - 0.5;
    int2 low = int2(floor(at));
    float2 within = at - float2(low);
    float2* cells = buffers()->foliagePushes;
    float2 total = 0.0;
    for (uint corner = 0; corner < 4; corner++)
    {
        int2 cell = low + int2(corner & 1, corner >> 1);
        int2 inGrid = cell - pushes.origin;
        if (any(inGrid < 0) || any(inGrid >= int(PUSH_CELLS)))
        {
            continue;
        }
        int2 slot = (cell % int(PUSH_CELLS) + int(PUSH_CELLS)) % int(PUSH_CELLS);
        float2 weights = lerp(1.0 - within, within, float2(corner & 1, corner >> 1));
        total += cells[slot.x + slot.y * PUSH_CELLS] * weights.x * weights.y;
    }
    return total;
}

// How far the wind, and what pushes through foliage, move a vertex at
// `world` of a plant standing at `origin`, `height` tall, drawn with
// `material`, its normal `normal` and its wind masks `masks`, at `time`.
float3 windOffset(Frame* frame, Material material, float3 origin, float height, float3 world, float3 normal, uint masks, float time)
{
    if (material.windBend <= 0.0 && material.windFlutter <= 0.0)
    {
        return 0.0;
    }
    float up = saturate((world.y - origin.y) / height);
    // Pushed aside by what passes through, bending from the ground as the wind bends it.
    // Read where each blade is, so a patch of grass parts round what passes through it.
    float2 pushed = foliagePush(frame, world);
    float3 offset = float3(pushed.x, 0.0, pushed.y) * (material.windBend * up * up * height * PUSH_LEAN);
    bool sea;
    float below = material.windBend > 0.0 ? plantUnderwater(frame, origin, sea) : 0.0;
    if (below > 0.0)
    {
        return bowed(offset + surgeOffset(frame, origin, height, below, sea, up, time) * material.windBend, height);
    }
    Wind wind = frame->wind;
    if (wind.strength <= 0.0)
    {
        return bowed(offset, height);
    }
    float3 along = float3(wind.direction.x, 0.0, wind.direction.y);
    float3 across = float3(-wind.direction.y, 0.0, wind.direction.x);
    // The gust over the plant: noise drifting downwind, so it sweeps across the level. Where the plant stands is
    // read from the level's middle, so gusts and sways stay where they are as the world's middle moves.
    float2 place = levelPosition(frame, origin, 4096.0).xz;
    float push = windPush(wind, place, time);
    // Each plant sways at its own phase, so they never move in step.
    float phase = windHash(floor(place * 4.0)) * 6.2831;
    float sway = sin(time * 1.6 + phase) * 0.6 + sin(time * 2.7 + phase * 1.7) * 0.4;
    float rock = sin(time * 2.1 + phase * 2.3);

    // Bent from the ground up: nothing at the root, the most at the tip.
    float lean = material.windBend * up * up * height * WIND_LEAN;
    offset += along * lean * max(push + wind.turbulence * wind.strength * sway * 0.5, 0.0);
    offset += across * lean * wind.turbulence * wind.strength * rock * 0.35;
    offset = bowed(offset, height);

    float flutters = float(masks & 0xFF) / 255.0;
    float own = float((masks >> 8) & 0xFF) / 255.0 * 6.2831;
    float branch = float((masks >> 16) & 0xFF) / 255.0;
    // A branch sways at its own pace on top of the plant's lean, downwind and bobbing up and down.
    float swing = sin(time * 2.4 + phase + own) * 0.6 + sin(time * 3.9 + (phase + own) * 1.3) * 0.4;
    float reach = branch * material.windBend * height * WIND_BRANCH * max(push, 0.0);
    offset += (along * (0.6 + 0.4 * swing) + float3(0.0, 0.5 * swing, 0.0)) * reach;

    // Leaves flutter along their faces, quickly and a little, more out at the tips.
    float flicker = dot(levelPosition(frame, world, 1024.0), float3(3.1, 2.3, 4.7)) + own;
    float flutter = sin(time * 13.0 + flicker) * 0.6 + sin(time * 21.0 + flicker * 1.7) * 0.4;
    offset += normal * flutter * flutters * material.windFlutter * WIND_FLUTTER * max(push, 0.0) * (0.3 + 0.7 * up);
    return offset;
}
