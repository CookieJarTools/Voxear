// Smoke (src/smoke.rs): clouds filling the space round their Objs, each a
// grid of cells the engine filled out from its middle against the level's
// colliders; and mists (src/mist.rs), trails of fog left behind moving
// Objs. Marched at half the view's size, lit by the sun through the cloud
// and by the light round it, then laid over the scene at full size, each
// pixel from the half-size ones nearest it in depth.

#include "particle_shared.slang"

// How many cells a smoke's grid has along each side (smoke::SMOKE_GRID).
static const uint SMOKE_GRID = 32;
// How many 32-bit words a smoke's cells take, and how many bricks of 4×4×4
// cells its grid has along each side, whose map follows its cells.
static const uint SMOKE_WORDS = SMOKE_GRID * SMOKE_GRID * SMOKE_GRID / 4;
static const uint SMOKE_BRICKS = 8;
// How many steps a view takes through the smoke at most, and towards the sun from each.
static const uint SMOKE_STEPS = 80;
static const uint SMOKE_SUN_STEPS = 3;

struct SmokeOut
{
    float4 position : SV_Position;
    float2 uv : TEXCOORD0;
};

[shader("vertex")]
SmokeOut smokeVertex(uint vertexId : SV_VertexID)
{
    SmokeOut o;
    float2 ndc = fullscreenNdc(vertexId);
    o.position = float4(ndc, 0.0, 1.0);
    o.uv = ndc * 0.5 + 0.5;
    return o;
}

// How filled cell `c` of `smoke` is now, from 0 to 1: nothing where the fill
// did not reach, and filling in from the middle as the smoke spreads, its
// front softened over two steps.
float smokeCell(Smoke smoke, int3 c)
{
    if (any(c < 0) || any(c >= int(SMOKE_GRID)))
    {
        return 0.0;
    }
    uint index = (uint(c.z) * SMOKE_GRID + uint(c.y)) * SMOKE_GRID + uint(c.x);
    uint step = (buffers()->smokeCells[smoke.cells + index / 4] >> ((index % 4) * 8)) & 0xFF;
    if (step == 0)
    {
        return 0.0;
    }
    float front = smoke.grown * (smoke.steps + 2.0);
    return saturate((front - float(step - 1)) * 0.5);
}

// The baked noise (smoke::noise_atlas, push.texture in the march): four
// tiling gradient noises, one to a channel, about 0.5 each, period 8 in
// `p`. Each slice of depth is a tile of the atlas, read twice and blended,
// so a read costs two filtered texture reads rather than a lattice of
// hashes.
static const float NOISE_SIDE = 64.0;
static const float NOISE_TILE = 66.0;
static const float NOISE_ATLAS = 528.0;
static const float NOISE_CELLS = 8.0;

float4 smokeNoise(float3 p)
{
    Texture2D atlas = textures[NonUniformResourceIndex(push.texture)];
    float3 t = frac(p / NOISE_CELLS) * NOISE_SIDE;
    float slice = floor(t.z);
    float next = fmod(slice + 1.0, NOISE_SIDE);
    let read = (float s) => atlas.SampleLevel(samplers[SAMPLER_LINEAR_CLAMP],
        (float2(fmod(s, 8.0), floor(s / 8.0)) * NOISE_TILE + 1.0 + t.xy + 0.5) / NOISE_ATLAS, 0.0);
    return lerp(read(slice), read(next), t.z - slice);
}

// Turns between octaves, so no octave's rows line up with another's.
static const float3x3 OCTAVE_TURN_A = float3x3(0.00, 0.80, 0.60, -0.80, 0.36, -0.48, -0.60, -0.48, 0.64);
static const float3x3 OCTAVE_TURN_B = float3x3(0.64, -0.48, -0.60, 0.48, 0.86, -0.12, 0.60, -0.18, 0.78);

// Three octaves of the body noise at `q`, from 0 to 1, about 0.5.
float smokeBody(float3 q)
{
    return smokeNoise(q).a * 0.55 + smokeNoise(mul(OCTAVE_TURN_A, q) * 2.3 + 5.3).a * 0.3
        + smokeNoise(mul(OCTAVE_TURN_B, q) * 5.1 + 2.1).a * 0.15;
}

// How a shot's and a blast's holes open and fill back in (smoke::shot_open
// and smoke::blast_open).
static const float SHOT_RADIUS = 0.5;
static const float2 SHOT_REFILL = float2(2.0, 5.0);
static const float2 BLAST_REFILL = float2(3.0, 10.0);
static const float BLAST_SHELL = 3.0;
static const float BLAST_LEFT = 0.02;
static const float BLAST_PUSH = 1.5;

// How much of `smoke` at `world` its holes leave: 1 untouched, near 0 where
// a shot or a blast has cleared it, more than 1 in the shell a fresh blast
// pushed it out into. `ragged` moves the holes' edges about.
float smokeLeft(Smoke smoke, float3 world, float ragged)
{
    float left = 1.0;
    for (uint i = 0; i < smoke.shots; i++)
    {
        float4 from = buffers()->smokeHoles[smoke.holes + i * 2];
        float3 to = buffers()->smokeHoles[smoke.holes + i * 2 + 1].xyz;
        float age = push.frame->time - from.w;
        float3 line = to - from.xyz;
        float along = saturate(dot(world - from.xyz, line) / max(dot(line, line), 1e-6));
        float off = distance(world, from.xyz + line * along) + ragged;
        float open = (1.0 - smoothstep(SHOT_RADIUS * 0.5, SHOT_RADIUS, off)) * (1.0 - smoothstep(SHOT_REFILL.x, SHOT_REFILL.y, age));
        left *= age >= 0.0 ? 1.0 - open : 1.0;
    }
    for (uint i = 0; i < smoke.blasts; i++)
    {
        uint row = smoke.holes + smoke.shots * 2 + i * 2;
        float4 centre = buffers()->smokeHoles[row];
        float radius = buffers()->smokeHoles[row + 1].x;
        float age = push.frame->time - centre.w;
        float off = distance(world, centre.xyz) + ragged;
        float inside = 1.0 - smoothstep(radius * 0.8, radius, off);
        float open = inside * (1.0 - pow(smoothstep(BLAST_REFILL.x, BLAST_REFILL.y, age), 1.8)) * (1.0 - BLAST_LEFT);
        // Pushed out into a thicker shell round it while it is fresh.
        float shell = exp(-pow((off - radius * 1.15) / max(radius * 0.2, 1e-3), 2.0));
        open -= shell * (1.0 - smoothstep(0.0, BLAST_SHELL, age)) * BLAST_PUSH;
        left *= age >= 0.0 ? 1.0 - open : 1.0;
    }
    return left;
}

// How thick `smoke` is at `world`, as light a metre takes: its cells blended
// smoothly, and with `swirls`, eaten into by drifting noise, most at its
// edges, so it billows rather than showing its cells.
float smokeDensity(Smoke smoke, float3 world, bool swirls)
{
    // Read a little off where it is, by drifting noise, so the cloud's edge does not follow its cells' rows; and
    // as it disperses, from lower down, so it rises, and more off, so it drifts apart.
    float gone = smoke.dispersed;
    if (swirls)
    {
        float3 w = world * 1.1 + smoke.seed * 0.37 + float3(0.0, -smoke.age * 0.2, 0.0);
        float3 warp = smokeNoise(w).rgb - 0.5;
        // Far enough, near a cell, that the cloud's edge follows no row of them.
        world += warp * smoke.cell * 2.4 - float3(0.0, 1.5 * gone * gone, 0.0);
        if (gone > 0.0)
        {
            // Drifting apart in broad, slow swirls, a metre or two across.
            float3 broad = world * 0.3 + smoke.seed * 0.11 + float3(smoke.age * 0.05, 0.0, 0.0);
            float3 apart = smokeNoise(broad + 7.7).rgb - 0.5;
            world += apart * 2.5 * gone;
        }
    }
    float3 g = (world - smoke.origin) / smoke.cell - 0.5;
    int3 c = int3(floor(g));
    float3 f = g - float3(c);
    // Blended smoothly from cell to cell, so no crease runs along their edges.
    f = f * f * f * (f * (f * 6.0 - 15.0) + 10.0);
    float filled = 0.0;
    for (uint k = 0; k < 8; k++)
    {
        int3 o = int3(k & 1, (k >> 1) & 1, (k >> 2) & 1);
        float3 w = lerp(1.0 - f, f, float3(o));
        filled += smokeCell(smoke, c + o) * w.x * w.y * w.z;
    }
    if (filled <= 1e-3)
    {
        return 0.0;
    }
    if (!swirls)
    {
        return filled * smoke.density * (1.0 - gone) * smokeLeft(smoke, world, 0.0);
    }
    float3 q = world * 0.45 + smoke.seed + float3(0.0, -smoke.age * 0.12, 0.0);
    float n = smokeBody(q);
    // The holes' edges ragged by the same swirls.
    // Thicker and thinner through and through, not only at its edge, so from inside it swirls past.
    // Dispersing, it breaks up from its thin swirls first into wisps, and those thin away.
    float swirled = saturate(filled * 1.7 - (1.0 - n) * (1.1 + 3.0 * gone)) * (0.3 + 1.4 * n * n) * (1.0 - gone * gone);
    return swirled * smoke.density * smokeLeft(smoke, world, (n - 0.5) * smoke.cell);
}

// The smokes' thickness at `world`, all of them together.
float smokesAt(Frame* frame, uint count, float3 world, bool swirls)
{
    float total = 0.0;
    for (uint i = 0; i < count; i++)
    {
        Smoke smoke = buffers()->smokes[i];
        float3 local = (world - smoke.origin) / (smoke.cell * float(SMOKE_GRID));
        if (any(local < 0.0) || any(local > 1.0))
        {
            continue;
        }
        // Nothing to read in a brick with no smoke in it or round it (smoke::SmokeFill::bricks).
        uint3 b = uint3(clamp(local * float(SMOKE_BRICKS), 0.0, float(SMOKE_BRICKS - 1)));
        uint bit = (b.z * SMOKE_BRICKS + b.y) * SMOKE_BRICKS + b.x;
        if (((buffers()->smokeCells[smoke.cells + SMOKE_WORDS + bit / 32] >> (bit % 32)) & 1) != 0)
        {
            total += smokeDensity(smoke, world, swirls);
        }
    }
    return total;
}

// How far each piece of a mist's trail rises as it ages, in metres a second (mist::MIST_RISE).
static const float MIST_RISE = 0.4;

// How thick `mist`'s trail is at `world` at `time`: round the nearest piece
// of its line, as wide as that piece has widened and as thick as it has
// yet to thin; with `swirls`, eaten into by drifting noise as smoke is,
// more as it ages, so it breaks into wisps.
float mistDensity(Mist mist, float3 world, float time, bool swirls)
{
    if (swirls)
    {
        // Swirls as fine as the trail is narrow.
        float3 w = world * (1.6 / mist.width) + mist.seed * 0.37 + float3(0.0, -time * 0.6, 0.0);
        float3 warp = smokeNoise(w).rgb - 0.5;
        world += warp * mist.width * 1.5;
    }
    // How far off the nearest piece, against how wide it is there, squared; and how far that piece has aged.
    float nearest = 1e9;
    float aged = 1.0;
    for (uint i = 0; i < mist.count; i++)
    {
        float4 a = buffers()->mistPoints[mist.points + i];
        float4 b = buffers()->mistPoints[mist.points + min(i + 1, mist.count - 1)];
        float3 from = a.xyz + float3(0.0, MIST_RISE * a.w, 0.0);
        float3 line = b.xyz + float3(0.0, MIST_RISE * b.w, 0.0) - from;
        float along = saturate(dot(world - from, line) / max(dot(line, line), 1e-6));
        float age = saturate(lerp(a.w, b.w, along) / mist.linger);
        float radius = mist.width * 0.5 * (1.0 + age);
        float3 off = world - (from + line * along);
        float share = dot(off, off) / (radius * radius);
        aged = share < nearest ? age : aged;
        nearest = min(nearest, share);
    }
    if (nearest >= 1.0)
    {
        return 0.0;
    }
    float shape = (1.0 - nearest) * pow(1.0 - aged, 1.5);
    if (!swirls)
    {
        return shape * mist.density;
    }
    float3 q = world * (2.2 / mist.width) + mist.seed + float3(0.0, -time * 0.9, 0.0);
    float n = smokeBody(q);
    // Eaten into from its edge most, and more as it ages, so it frays into wisps.
    // Whole where it was left last, round whatever is in it, and fraying fast as it ages.
    float fray = lerp(0.35, 1.25, saturate(aged * 4.0)) + 1.8 * aged;
    float frayed = saturate((1.0 - nearest) * 2.6 - (1.0 - n) * fray) * pow(1.0 - aged, 1.5);
    return frayed * (0.3 + 1.6 * n * n) * mist.density;
}

// The mists' thickness at `world`, all of them together.
float mistsAt(Frame* frame, uint count, float3 world, bool swirls)
{
    float total = 0.0;
    for (uint i = 0; i < count; i++)
    {
        Mist mist = buffers()->mists[i];
        if (all(world >= mist.low) && all(world <= mist.high))
        {
            total += mistDensity(mist, world, frame->time, swirls);
        }
    }
    return total;
}

// Henyey–Greenstein: how much light going along one way scatters by `cosine` off it.
float smokePhase(float cosine, float g)
{
    float g2 = g * g;
    return (1.0 - g2) / (4.0 * PI * pow(max(1.0 + g2 - 2.0 * g * cosine, 1e-4), 1.5));
}

struct SmokeMarched
{
    // The light the smokes send towards the eye, and in alpha how much shows through them.
    float4 light : SV_Target0;
    // How far into them, on average, their light comes from: where they are, for last frame's to be found.
    float distance : SV_Target1;
};

// Which of each 2×2 of half-size pixels the march works out this frame,
// one in turn over four frames.
int2 smokeTurn(Frame* frame)
{
    uint k = frame->frameIndex % 4;
    return k == 0 ? int2(0, 0) : (k == 1 ? int2(1, 1) : (k == 2 ? int2(1, 0) : int2(0, 1)));
}

// The smokes along this pixel's view, as far as the scene behind: the light
// they send towards the eye, fogged as far off as they start, and in alpha
// how much of what lies behind shows through them. push.textures: x the
// scene's depth, y the fog's volume, z how many smokes, w how many mists.
[shader("fragment")]
SmokeMarched smokeMarchFragment(SmokeOut input)
{
    SmokeMarched none = { float4(0.0, 0.0, 0.0, 1.0), 0.0 };
    Frame* frame = push.frame;
    uint count = push.textures.z;
    uint mists = push.textures.w;
    // Marched a quarter of the half-size pixels a frame, a different one of each 2×2 each frame (push.size the
    // half size), the resolve putting them together.
    float2 halfPixel = float2(int2(input.position.xy) * 2 + smokeTurn(frame)) + 0.5;
    float2 uv = halfPixel / float2(push.size);
    float4 nearH = transform(frame->invViewProj, float4(uv * 2.0 - 1.0, 1.0, 1.0));
    float3 eye = frame->cameraPosition;
    float3 way = normalize(nearH.xyz / nearH.w - eye);
    float behind = textures[NonUniformResourceIndex(push.textures.x)].Load(int3(int2(uv * float2(frame->viewport)), 0)).r;
    float scene = behind > 0.0 ? frame->near / behind / max(dot(way, frame->cameraForward), 1e-3) : 1e9;

    // Where the view is inside any smoke's grid, and the finest cells it meets.
    float enter = 1e9;
    float leave = 0.0;
    float cell = 1e9;
    float3 color = 0.8;
    for (uint i = 0; i < count; i++)
    {
        Smoke smoke = buffers()->smokes[i];
        float3 low = (smoke.origin - eye) / way;
        float3 high = (smoke.origin + smoke.cell * float(SMOKE_GRID) - eye) / way;
        float near = max(max(min(low.x, high.x), min(low.y, high.y)), min(low.z, high.z));
        float far = min(min(max(low.x, high.x), max(low.y, high.y)), max(low.z, high.z));
        if (far > max(near, 0.0))
        {
            color = near < enter ? smoke.color : color;
            enter = min(enter, near);
            leave = max(leave, far);
            cell = min(cell, smoke.cell);
        }
    }
    for (uint i = 0; i < mists; i++)
    {
        Mist mist = buffers()->mists[i];
        float3 low = (mist.low - eye) / way;
        float3 high = (mist.high - eye) / way;
        float near = max(max(min(low.x, high.x), min(low.y, high.y)), min(low.z, high.z));
        float far = min(min(max(low.x, high.x), max(low.y, high.y)), max(low.z, high.z));
        if (far > max(near, 0.0))
        {
            color = near < enter ? mist.color : color;
            enter = min(enter, near);
            leave = max(leave, far);
            cell = min(cell, mist.width * 0.5);
        }
    }
    enter = max(enter, 0.0);
    leave = min(leave, scene);
    if (enter >= leave)
    {
        return none;
    }
    float step = max(cell * 0.5, (leave - enter) / float(SMOKE_STEPS));
    // Where in each step it samples, different for each pixel and frame and with no pattern across the view,
    // so over frames it averages to the smooth whole.
    uint2 seed = uint2(input.position.xy);
    uint mixed = (seed.x * 1973u + seed.y * 9277u + frame->frameIndex * 26699u) | 1u;
    mixed ^= mixed >> 16;
    mixed *= 0x7feb352du;
    mixed ^= mixed >> 15;
    mixed *= 0x846ca68bu;
    mixed ^= mixed >> 16;
    float jitter = float(mixed & 0xFFFFFFu) / 16777216.0;

    bool atmosphere = frame->lighting.sky == SKY_ATMOSPHERE;
    float3 toSun = -frame->sunDirection;
    float3 sunLight = atmosphere ? sunThroughAtmosphere(frame) : frame->sunColor;
    float cosine = dot(way, toSun);
    float phase = lerp(smokePhase(cosine, -0.2), smokePhase(cosine, 0.55), 0.6);
    float3 ambient = atmosphere ? atmosphereAmbient(frame, 0.5, sunLight)
        : lerp(frame->groundColor, lerp(frame->skyHorizon, frame->skyZenith, 0.5), 0.5) * 0.35 * frame->ambient;
    if (frame->ddgi.enabled != 0)
    {
        ambient = ddgiLightAround(frame, eye + way * enter, -way, ambient);
    }

    float through = 1.0;
    float3 light = 0.0;
    float sun = 1.0;
    bool sunSeen = false;
    float first = -1.0;
    float weighed = 0.0;
    float weights = 0.0;
    for (uint i = 0; i < SMOKE_STEPS; i++)
    {
        // Each step's stretch counted for as long as it is, the last cut short where the scene or the smoke
        // ends; the jitter moves only where in it the sample is, so what stands close behind thin smoke shows
        // no pattern of steps.
        float from = enter + float(i) * step;
        if (from >= leave || through < 0.01)
        {
            break;
        }
        float stretch = min(step, leave - from);
        float t = from + jitter * stretch;
        float3 at = eye + way * t;
        // Thinner right by the eye, so from inside the view does not begin at a wall of it.
        float thick = (smokesAt(frame, count, at, true) + mistsAt(frame, mists, at, true)) * smoothstep(0.0, 0.8, t);
        if (thick < 1e-3)
        {
            continue;
        }
        first = first < 0.0 ? t : first;
        // The sun's light through the cloud between it and here, and past what shadows it: every other step, as
        // it changes slowly along the view, and on the first.
        if ((i & 1) == 0 || !sunSeen)
        {
            float toward = 0.0;
            for (uint k = 1; k <= SMOKE_SUN_STEPS; k++)
            {
                float3 sunward = at + toSun * cell * 1.5 * float(k);
                toward += smokesAt(frame, count, sunward, false) + mistsAt(frame, mists, sunward, false);
            }
            // Looked up a little off where it is, differently each frame and step, so the shadows of what stands
            // in the sun soften over frames rather than stepping with the shadow map's texels.
            float3 shake = float3(frac(jitter * 7.13 + float(i) * 0.618), frac(jitter * 3.71 + float(i) * 0.382), frac(jitter * 5.37)) - 0.5;
            sun = exp(-toward * cell * 1.5) * sunShadowHard(frame, at + shake * cell, toSun);
            sunSeen = true;
        }
        // The lamps in this pixel's cluster, scattered as evenly every way.
        float3 lamps = 0.0;
        uint base = clusterAt(frame, halfPixel * 2.0, t * dot(way, frame->cameraForward)) * (frame->clusterMaxLights + 1);
        for (uint l = 0; l < buffers()->clusters[base]; l++)
        {
            Light lamp = buffers()->lights[buffers()->clusters[base + 1 + l]];
            lamps += localLightInput(lamp, at, normalizeOr(lamp.position - at, way)).color;
        }
        float3 lit = color * (sunLight * sun * phase * PI + lamps * 0.25 + ambient);
        float kept = exp(-thick * stretch);
        light += through * lit * (1.0 - kept);
        weighed += t * through * (1.0 - kept);
        weights += through * (1.0 - kept);
        through *= kept;
    }
    if (first < 0.0)
    {
        return none;
    }
    float3 start = eye + way * first;
    float4 air = particleFog(frame, start, transform(frame->viewProj, float4(start, 1.0)));
    SmokeMarched marched = { float4(light * air.a + air.rgb * (1.0 - through), through), weighed / max(weights, 1e-6) };
    return marched;
}

// This frame's marched smoke (push.texture) blended with last frame's
// (push.textures.y, when push.textures.z says it was drawn), found where
// this pixel's smoke was then from how far into it its light comes
// (push.textures.x), and kept within the colours round it now, so the
// smoke stays smooth as it swirls and the camera moves but leaves no
// trail.
[shader("fragment")]
float4 smokeResolveFragment(SmokeOut input) : SV_Target0
{
    Frame* frame = push.frame;
    Texture2D marched = textures[NonUniformResourceIndex(push.texture)];
    Texture2D into = textures[NonUniformResourceIndex(push.textures.x)];
    int2 pixel = int2(input.position.xy);
    int2 quarter = int2(push.size);
    int2 q = min(pixel / 2, quarter - 1);
    // This frame's smoke here: as marched, for the pixel marched this frame; the others blended from the four
    // marched round them.
    int2 turn = smokeTurn(frame);
    float2 g = (float2(pixel) - float2(turn)) * 0.5;
    int2 base = int2(floor(g));
    float2 f = g - float2(base);
    float4 fresh = 0.0;
    for (uint k = 0; k < 4; k++)
    {
        int2 o = int2(k & 1, k >> 1);
        float2 w = lerp(1.0 - f, f, float2(o));
        fresh += marched.Load(int3(clamp(base + o, int2(0, 0), quarter - 1), 0)) * (w.x * w.y);
    }
    // Kept within the colours of the marched pixels round it, as far as any of them reaches into smoke.
    float4 low = fresh;
    float4 high = fresh;
    float far = into.Load(int3(q, 0)).r;
    for (int y = -1; y <= 1; y++)
    {
        for (int x = -1; x <= 1; x++)
        {
            int2 round = clamp(q + int2(x, y), int2(0, 0), quarter - 1);
            float4 there = marched.Load(int3(round, 0));
            low = min(low, there);
            high = max(high, there);
            far = far > 0.0 ? far : into.Load(int3(round, 0)).r;
        }
    }
    if (push.textures.z == 0 || far <= 0.0)
    {
        return fresh;
    }
    float4 nearH = transform(frame->invViewProj, float4(input.uv * 2.0 - 1.0, 1.0, 1.0));
    float3 way = normalize(nearH.xyz / nearH.w - frame->cameraPosition);
    float4 before = transform(frame->prevViewProj, float4(frame->cameraPosition + way * far, 1.0));
    float2 then = before.xy / before.w * 0.5 + 0.5;
    if (before.w <= 0.0 || any(then < 0.0) || any(then > 1.0))
    {
        return fresh;
    }
    float4 kept = textures[NonUniformResourceIndex(push.textures.y)].SampleLevel(samplers[SAMPLER_LINEAR_CLAMP], then, 0.0);
    // Well past the colours round it, so the march's step jitter, different each frame, averages away over
    // frames rather than being clamped back in; only last frame's that is plainly something else, as smoke a
    // moving camera has left behind, is pulled back.
    float4 slack = (high - low) + 0.1;
    return lerp(fresh, clamp(kept, low - slack, high + slack), 0.9);
}

// The scene's depth along the view at full size, in metres; far for the sky.
float smokeSceneDepth(Frame* frame, int2 pixel)
{
    float depth = textures[NonUniformResourceIndex(push.textures.x)].Load(int3(pixel, 0)).r;
    return depth > 0.0 ? frame->near / depth : 1e4;
}

// The marched smoke (push.texture, push.size across) laid over the scene:
// each pixel from the four half-size ones round it. Where what the scene
// has there stands in front of the smoke, weighted most to those whose
// scene lies as deep as its own, so the smoke's edge follows it sharply;
// where the scene lies behind the smoke, blended evenly, so its edges do
// not show through. push.textures.y is how far into the smoke its light
// comes from.
[shader("fragment")]
float4 smokeCompositeFragment(SmokeOut input) : SV_Target0
{
    Frame* frame = push.frame;
    Texture2D marched = textures[NonUniformResourceIndex(push.texture)];
    Texture2D into = textures[NonUniformResourceIndex(push.textures.y)];
    int2 half = int2(push.size);
    float own = smokeSceneDepth(frame, int2(input.position.xy));
    float2 at = input.uv * float2(half) - 0.5;
    int2 base = int2(floor(at));
    float2 f = at - float2(base);
    float4 total = 0.0;
    float weights = 0.0;
    for (uint k = 0; k < 4; k++)
    {
        int2 o = int2(k & 1, k >> 1);
        int2 c = clamp(base + o, int2(0, 0), half - 1);
        float2 centre = (float2(c) + 0.5) / float2(half);
        float theirs = smokeSceneDepth(frame, int2(centre * float2(frame->viewport)));
        float2 w = lerp(1.0 - f, f, float2(o));
        float smoke = into.Load(int3(c / 2, 0)).r;
        bool before = smoke > 0.0 && min(theirs, own) < smoke;
        float weight = w.x * w.y / (before ? 1e-3 + abs(theirs - own) / max(own, 1e-3) : 1.0);
        total += marched.Load(int3(c, 0)) * weight;
        weights += weight;
    }
    float4 smoke = total / max(weights, 1e-6);
    return float4(smoke.rgb, 1.0 - smoke.a);
}
