// Smoke (src/smoke.rs): clouds filling the space round their Objs, each a
// grid of cells the engine filled out from its middle against the level's
// colliders; and mists (src/mist.rs), trails of fog left behind moving
// Objs. Marched at half the view's size, lit by the sun through the cloud
// and by the light round it, then laid over the scene at full size, each
// pixel from the half-size ones nearest it in depth.

#include "particle_shared.slang"

// How many cells a smoke's grid has along each side (smoke::SMOKE_GRID).
static const uint SMOKE_GRID = 32;
// How many 32-bit words a smoke's cells take, two to a word, and how many
// bricks of 4×4×4 cells its grid has along each side, whose map follows its
// cells.
static const uint SMOKE_WORDS = SMOKE_GRID * SMOKE_GRID * SMOKE_GRID / 2;
static const uint SMOKE_BRICKS = 8;
// How many steps a view takes through the smoke at most, and towards the sun from each.
static const uint SMOKE_STEPS = 80;
static const uint SMOKE_SUN_STEPS = 3;
// How many cells towards the sun each of its looks is, and how long a stretch it stands for.
static const float SUN_AT[SMOKE_SUN_STEPS] = { 1.0, 3.5, 8.5 };
static const float SUN_ALONG[SMOKE_SUN_STEPS] = { 2.0, 3.5, 6.0 };
// How many times over the sun's light is counted scattering in the cloud.
static const uint SMOKE_ORDERS = 3;
// How near the eye, in metres, the smoke thins away to nothing. What stands
// nearer, as a gun held up, the march goes on past, so the smoke round it
// has no hole in it for its edge to show in as it moves; over it is laid
// only the thin smoke between it and the eye.
static const float SMOKE_NEAR = 1.0;
// How a mist shades itself and billows (Smoke::self_shadow, Smoke::billows).
static const float MIST_SELF_SHADOW = 0.5;
static const float MIST_BILLOWS = 1.0;

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
// did not reach, filling in from the middle as the smoke spreads, its front
// soft as it bursts out and sharper as it settles, and thinning out towards open air over the
// outer softness of its depth, so its edge fades away (Smoke::filled). Each
// cell is how many steps out it is in its low byte, and how deep inside the
// smoke in its high one.
float smokeCell(Smoke smoke, int3 c)
{
    if (any(c < 0) || any(c >= int(SMOKE_GRID)))
    {
        return 0.0;
    }
    uint index = (uint(c.z) * SMOKE_GRID + uint(c.y)) * SMOKE_GRID + uint(c.x);
    uint cell = (buffers()->smokeCells[smoke.cells + index / 2] >> ((index % 2) * 16)) & 0xFFFF;
    uint step = cell & 0xFF;
    if (step == 0)
    {
        return 0.0;
    }
    float front = smoke.grown * (smoke.steps + 2.0);
    float width = max(smoke.softness * 0.5 * (smoke.steps + 2.0) * (1.0 - smoke.grown), 2.0);
    float spread = saturate((front - float(step - 1)) / width);
    float deep = saturate(float(cell >> 8) / 255.0 / max(smoke.softness, 1e-3));
    return spread * deep * deep * (3.0 - 2.0 * deep);
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

// Three octaves of the body's billows at `q`, from 0 to 1, about 0.5: big
// round puffs, smaller puffs on those, and fine ones on those.
float smokeBody(float3 q)
{
    return smokeNoise(q).a * 0.55 + smokeNoise(mul(OCTAVE_TURN_A, q) * 2.3 + 5.3).a * 0.3
        + smokeNoise(mul(OCTAVE_TURN_B, q) * 5.1 + 2.1).a * 0.15;
}

// The same three octaves at `q`, read once for both: in x the billows, as
// smokeBody; in y smooth swirls with no puffs in them, as fog has, spread
// as wide about 0.5 as a single octave is.
float2 smokeOctaves(float3 q)
{
    float4 a = smokeNoise(q);
    float4 b = smokeNoise(mul(OCTAVE_TURN_A, q) * 2.3 + 5.3);
    float4 c = smokeNoise(mul(OCTAVE_TURN_B, q) * 5.1 + 2.1);
    float billows = a.a * 0.55 + b.a * 0.3 + c.a * 0.15;
    float swirls = a.r * 0.55 + b.g * 0.3 + c.b * 0.15;
    return float2(billows, 0.5 + (swirls - 0.5) * 1.55);
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
// edges, so it frays into wisps, or billows, rather than showing its cells.
float smokeDensity(Smoke smoke, float3 world, bool swirls)
{
    // Read a little off where it is, by drifting noise, so the cloud's edge does not follow its cells' rows; and
    // as it disperses, from lower down, so it rises, and more off, so it drifts apart. Its holes stay where the
    // shots and blasts went.
    float3 at = world;
    float gone = smoke.dispersed;
    if (swirls)
    {
        // Its swirls are its own, read from its corner, so they stay with it as the world's middle moves.
        float3 own = world - smoke.origin;
        float3 w = own * 1.1 + smoke.seed * 0.37 + float3(0.0, -smoke.age * 0.2, 0.0);
        float3 warp = smokeNoise(w).rgb - 0.5;
        // Far enough, near a cell, that the cloud's edge follows no row of them, but for billows not so far the
        // puffs smear; for wisps, farther, drawing its edge out into curling strands.
        float reach = smoke.cell * 2.0 + smoke.wisps * (1.0 - smoke.billows) * 1.2;
        // Never read from much lower, though, so it still lies on the floor, which it reaches a cell into.
        float3 moved = warp * reach;
        moved.y = max(moved.y, -0.75 * smoke.cell);
        world += moved - float3(0.0, 1.5 * gone * gone, 0.0);
        if (gone > 0.0)
        {
            // Drifting apart in broad, slow swirls, a metre or two across.
            float3 broad = own * 0.3 + smoke.seed * 0.11 + float3(smoke.age * 0.05, 0.0, 0.0);
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
        return filled * smoke.density * (1.0 - gone) * smokeLeft(smoke, at, 0.0);
    }
    // Rolling slowly upward and over, as a fresh cloud churns.
    float3 q = (world - smoke.origin) * 0.45 + smoke.seed + float3(0.0, -smoke.age * 0.12, 0.0);
    float2 octaves = smokeOctaves(q);
    // Billows: eaten into between its puffs at its edge, so its outline is round billows; thick through and
    // through, a little thinner between the puffs, so from inside it swirls past.
    float b = octaves.x;
    float puffs = saturate(filled * 2.0 - (1.0 - b) * (1.5 + 3.0 * gone)) * (0.5 + 1.2 * b * b);
    // Fog: thicker and thinner in drifting strands where it has thinned, so its thin edge frays into wisps with
    // haze between them while its middle stays whole, and only a little thicker and thinner through, so it is
    // even inside. Dispersing, the strands part.
    float s = octaves.y;
    float strands = (1.0 + 5.0 * smoke.wisps) * (1.0 - filled) + 6.0 * gone;
    float fog = filled * saturate(1.0 + (s - 0.5) * strands - gone) * (0.85 + 0.3 * s);
    // Dispersing, it breaks up from its thin swirls first into wisps, and those thin away.
    float swirled = lerp(fog, puffs, smoke.billows) * (1.0 - gone * gone);
    // The holes' edges ragged by the same swirls.
    float n = lerp(s, b, smoke.billows);
    return swirled * smoke.density * smokeLeft(smoke, at, (n - 0.5) * smoke.cell);
}

// Whether `smoke` may have any smoke at `world`: inside its grid, and in a
// brick with smoke in it or round it (smoke::SmokeFill::bricks).
bool smokeNear(Smoke smoke, float3 world)
{
    float3 local = (world - smoke.origin) / (smoke.cell * float(SMOKE_GRID));
    if (any(local < 0.0) || any(local > 1.0))
    {
        return false;
    }
    uint3 b = uint3(clamp(local * float(SMOKE_BRICKS), 0.0, float(SMOKE_BRICKS - 1)));
    uint bit = (b.z * SMOKE_BRICKS + b.y) * SMOKE_BRICKS + b.x;
    return ((buffers()->smokeCells[smoke.cells + SMOKE_WORDS + bit / 32] >> (bit % 32)) & 1) != 0;
}

// The smokes' thickness at `world`, all of them together.
float smokesAt(Frame* frame, uint count, float3 world, bool swirls)
{
    float total = 0.0;
    for (uint i = 0; i < count; i++)
    {
        Smoke smoke = buffers()->smokes[i];
        if (smokeNear(smoke, world))
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
float mistDensity(Frame* frame, Mist mist, float3 world, float time, bool swirls)
{
    // Its swirls are read from the level's middle, so they stay where they are as the world's middle moves.
    float3 level = levelPosition(frame, world, 1024.0);
    if (swirls)
    {
        // Swirls as fine as the trail is narrow.
        float3 w = level * (1.6 / mist.width) + mist.seed * 0.37 + float3(0.0, -time * 0.6, 0.0);
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
    float3 q = level * (2.2 / mist.width) + mist.seed + float3(0.0, -time * 0.9, 0.0);
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
            total += mistDensity(frame, mist, world, frame->time, swirls);
        }
    }
    return total;
}

// How the smokes and mists at a point look together: how thick they are,
// and their colour, how dark they shade themselves and how much they
// billow, each of those weighed by how thick each is there, so where one
// drifts through another each keeps its own look.
struct SmokeLook
{
    float thick;
    float3 color;
    float selfShadow;
    float billows;
};

SmokeLook smokeLookAt(Frame* frame, uint count, uint mists, float3 world)
{
    SmokeLook look = { 0.0, float3(0.0, 0.0, 0.0), 0.0, 0.0 };
    for (uint i = 0; i < count; i++)
    {
        Smoke smoke = buffers()->smokes[i];
        if (smokeNear(smoke, world))
        {
            float thick = smokeDensity(smoke, world, true);
            look.thick += thick;
            look.color += smoke.color * thick;
            look.selfShadow += smoke.selfShadow * thick;
            look.billows += smoke.billows * thick;
        }
    }
    for (uint i = 0; i < mists; i++)
    {
        Mist mist = buffers()->mists[i];
        if (all(world >= mist.low) && all(world <= mist.high))
        {
            float thick = mistDensity(frame, mist, world, frame->time, true);
            look.thick += thick;
            look.color += mist.color * thick;
            look.selfShadow += MIST_SELF_SHADOW * thick;
            look.billows += MIST_BILLOWS * thick;
        }
    }
    float share = 1.0 / max(look.thick, 1e-6);
    look.color *= share;
    look.selfShadow *= share;
    look.billows *= share;
    return look;
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
    float depth = behind > 0.0 ? frame->near / behind : 1e9;
    float scene = depth >= SMOKE_NEAR ? depth / max(dot(way, frame->cameraForward), 1e-3) : 1e9;

    // Where the view is inside any smoke's grid, and the finest cells it meets.
    float enter = 1e9;
    float leave = 0.0;
    float cell = 1e9;
    for (uint i = 0; i < count; i++)
    {
        Smoke smoke = buffers()->smokes[i];
        float3 low = (smoke.origin - eye) / way;
        float3 high = (smoke.origin + smoke.cell * float(SMOKE_GRID) - eye) / way;
        float near = max(max(min(low.x, high.x), min(low.y, high.y)), min(low.z, high.z));
        float far = min(min(max(low.x, high.x), max(low.y, high.y)), max(low.z, high.z));
        if (far > max(near, 0.0))
        {
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
    // Light scattered once, a little more forward than back; and each time again, spread more evenly every way.
    float phases[SMOKE_ORDERS];
    for (uint o = 0; o < SMOKE_ORDERS; o++)
    {
        float spread = pow(0.5, float(o));
        phases[o] = lerp(smokePhase(cosine, -0.2 * spread), smokePhase(cosine, 0.55 * spread), 0.6);
    }
    float3 ambient = atmosphere ? atmosphereAmbient(frame, 0.5, sunLight)
        : lerp(frame->groundColor, lerp(frame->skyHorizon, frame->skyZenith, 0.5), 0.5) * 0.35 * frame->ambient;
    if (frame->ddgi.enabled != 0)
    {
        ambient = ddgiLightAround(frame, eye + way * enter, -way, ambient);
    }

    float through = 1.0;
    float3 light = 0.0;
    float sun = 1.0;
    float sky = 1.0;
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
        SmokeLook look = smokeLookAt(frame, count, mists, at);
        float thick = look.thick * smoothstep(0.0, SMOKE_NEAR, t);
        if (thick < 1e-3)
        {
            continue;
        }
        first = first < 0.0 ? t : first;
        // The sun's light through the cloud between it and here, and past what shadows it: every other step, as
        // it changes slowly along the view, and on the first.
        if ((i & 1) == 0 || !sunSeen)
        {
            // Looked for farther apart the farther off, so deep in the cloud a gap between puffs close by does not
            // let the sun in as though from outside, and a little nearer or farther each frame, so no bands show.
            float toward = 0.0;
            float reach = 0.8 + 0.4 * frac(jitter * 13.7 + float(i) * 0.29);
            for (uint k = 0; k < SMOKE_SUN_STEPS; k++)
            {
                // The nearest through the swirled puffs, as it decides where light meets shade, so the cells'
                // boxes do not show in the shading; the farther, which only darken, through the cells.
                float3 sunward = at + toSun * cell * SUN_AT[k] * reach;
                float thick = smokesAt(frame, count, sunward, k == 0) + mistsAt(frame, mists, sunward, false);
                toward += thick * SUN_ALONG[k] * reach;
            }
            // Looked up a little off where it is, differently each frame and step, so the shadows of what stands
            // in the sun soften over frames rather than stepping with the shadow map's texels.
            float3 shake = float3(frac(jitter * 7.13 + float(i) * 0.618), frac(jitter * 3.71 + float(i) * 0.382), frac(jitter * 5.37)) - 0.5;
            float depth = toward * cell;
            float shadow = sunShadowHard(frame, at + shake * cell, toSun);
            // How much of the light each order of scattering keeps, and how much less of the cloud each sees on its
            // way in, as the light goes round what stands between: the less it shades itself, the more it keeps
            // and the deeper it reaches, so it is lit right through, as fog in daylight.
            float keeps = lerp(0.75, 0.45, look.selfShadow);
            float sees = lerp(0.2, 0.5, look.selfShadow);
            // The sun's light scattered once and over again: each order reaches deeper and comes out more evenly
            // every way, so the lit side is bright from any side and the far side shaded rather than black.
            sun = 0.0;
            float share = 1.0;
            float seen = 1.0;
            for (uint o = 0; o < SMOKE_ORDERS; o++)
            {
                // The last order stands for itself and every one after it.
                float weight = o + 1 < SMOKE_ORDERS ? share : share / (1.0 - keeps);
                sun += weight * phases[o] * exp(-depth * seen);
                share *= keeps;
                seen *= sees;
            }
            sun *= shadow * PI;
            // Under its own puffs, the sky's light is less: looked for a cell or a few above, a different height
            // each frame, so it averages out rather than streaking down, and through the swirled puffs, not the
            // cells, whose boxes would show in it. Scattered on through the cloud, some of it still reaches in.
            float lift = cell * (1.0 + 2.5 * frac(jitter * 11.3 + float(i) * 0.47));
            float over = smokesAt(frame, count, at + float3(0.0, lift, 0.0), true);
            sky = lerp(lerp(0.85, 0.35, look.selfShadow), 1.0, exp(-over * lift * 0.8));
            sunSeen = true;
        }
        // The lamps in this pixel's cluster, scattered as evenly every way.
        float3 lamps = 0.0;
        uint base = clusterAt(frame, halfPixel * 2.0, t * dot(way, frame->cameraForward)) * (frame->clusterMaxLights + 1);
        for (uint l = 0; l < buffers()->clusters[base]; l++)
        {
            Light lamp = buffers()->lights[buffers()->clusters[base + 1 + l]];
            lamps += localLightInput(lamp, at, normalizeOr(lamp.position - at, way), false).color;
        }
        // Billowing, its thin edges catch less of the sun than its body does, so each puff is darker round its rim
        // and stands out from the one behind.
        float body = 1.0 - exp(-thick * cell * 2.0);
        float rim = lerp(1.0, body, 0.6 * look.billows * saturate(0.5 - 0.5 * cosine));
        float3 sunlit = sunLight * sun * rim;
        float3 lit = look.color * (sunlit + lamps * 0.25 + ambient * sky);
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

// The scene's depth along the view at full-size `pixel`, from depth texture
// `texture`, in metres; far for the sky.
float smokeSceneDepth(Frame* frame, uint texture, int2 pixel)
{
    float depth = textures[NonUniformResourceIndex(texture)].Load(int3(pixel, 0)).r;
    return depth > 0.0 ? frame->near / depth : 1e4;
}

// How far the march went along the view at full-size `pixel`: to the scene,
// or on past what stands nearer than SMOKE_NEAR, as far as the sky.
float smokeMarchedDepth(Frame* frame, uint texture, int2 pixel)
{
    float scene = smokeSceneDepth(frame, texture, pixel);
    return scene < SMOKE_NEAR ? 1e4 : scene;
}

// Whether something stands in front of the smoke, whose light comes from
// `smoke` metres in, at a pixel whose scene lies `own` metres off or at one
// whose scene lies `theirs` off, as a post standing before it. With no
// smoke found round about, as where every pixel marched round a post's edge
// fell on the post, whatever stands there may be.
bool smokeBefore(float own, float theirs, float smoke)
{
    return smoke <= 0.0 || min(theirs, own) < smoke;
}

// How much a pixel whose scene lies `theirs` metres off stands for one whose
// scene lies `own` off: alike where both lie behind the smoke, so the
// scene's edges do not show through it; where something stands in front of
// it, as far as their scenes lie as deep, so the smoke's edge follows what
// stands before it, as a post, sharply rather than in half-size blocks.
float smokeAlike(float own, float theirs, float smoke)
{
    return smokeBefore(own, theirs, smoke) ? 1.0 / (1e-3 + abs(theirs - own) / max(own, 1e-3)) : 1.0;
}

// This frame's marched smoke (push.texture) blended with last frame's
// (push.textures.y, when push.textures.z says it was drawn), found where
// this pixel's smoke was then from how far into it its light comes
// (push.textures.x), and kept within the colours round it now, so the
// smoke stays smooth as it swirls and the camera moves but leaves no
// trail. Each pixel is made from those marched round it whose scene lies
// as deep (push.textures.w the scene's depth), so a post standing before the
// smoke keeps none of it, and the smoke beside the post none of the post's.
[shader("fragment")]
float4 smokeResolveFragment(SmokeOut input) : SV_Target0
{
    Frame* frame = push.frame;
    Texture2D marched = textures[NonUniformResourceIndex(push.texture)];
    Texture2D into = textures[NonUniformResourceIndex(push.textures.x)];
    int2 pixel = int2(input.position.xy);
    int2 quarter = int2(push.size);
    int2 q = min(pixel / 2, quarter - 1);
    int2 turn = smokeTurn(frame);
    // The scene's depth at the middle of a half-size pixel.
    int2 half = int2((frame->viewport + 1) / 2);
    let depthAt = (int2 p) => smokeMarchedDepth(frame, push.textures.w, int2((float2(p) + 0.5) / float2(half) * float2(frame->viewport)));
    float own = depthAt(pixel);
    // This frame's smoke here: as marched, for the pixel marched this frame; the others blended from the four
    // marched round them, those whose scene lies as deep counted most.
    float2 g = (float2(pixel) - float2(turn)) * 0.5;
    int2 base = int2(floor(g));
    float2 f = g - float2(base);
    float smoke = 0.0;
    for (uint k = 0; k < 4; k++)
    {
        smoke = max(smoke, into.Load(int3(clamp(base + int2(k & 1, k >> 1), int2(0, 0), quarter - 1), 0)).r);
    }
    // Whether a marched pixel's scene lies about as deep as this one's, or both lie behind the smoke.
    let same = (float theirs) => !smokeBefore(own, theirs, smoke) || abs(theirs - own) <= 0.25 * own;
    float4 fresh = 0.0;
    float weights = 0.0;
    bool found = false;
    for (uint k = 0; k < 4; k++)
    {
        int2 o = int2(k & 1, k >> 1);
        int2 c = clamp(base + o, int2(0, 0), quarter - 1);
        float theirs = depthAt(c * 2 + turn);
        float2 w = lerp(1.0 - f, f, float2(o));
        float weight = w.x * w.y * smokeAlike(own, theirs, smoke);
        fresh += marched.Load(int3(c, 0)) * weight;
        weights += weight;
        found = found || same(theirs);
    }
    fresh /= max(weights, 1e-6);
    if (!found)
    {
        // By the edge of something standing before the smoke, none of the four marched round it lies as deep, as
        // smoke just beside a post whose four all fell on the post: from those a little farther round that do.
        float4 near = 0.0;
        float nears = 0.0;
        for (int y = -1; y <= 2; y++)
        {
            for (int x = -1; x <= 2; x++)
            {
                int2 c = clamp(base + int2(x, y), int2(0, 0), quarter - 1);
                if (same(depthAt(c * 2 + turn)))
                {
                    near += marched.Load(int3(c, 0));
                    nears += 1.0;
                }
            }
        }
        fresh = nears > 0.0 ? near / nears : fresh;
    }
    // Kept within the colours of the marched pixels round it whose scene lies about as deep, as far as any of
    // them reaches into smoke.
    float4 low = fresh;
    float4 high = fresh;
    float far = into.Load(int3(q, 0)).r;
    bool edge = false;
    for (int y = -1; y <= 1; y++)
    {
        for (int x = -1; x <= 1; x++)
        {
            int2 round = clamp(q + int2(x, y), int2(0, 0), quarter - 1);
            float theirs = depthAt(round * 2 + turn);
            if (smokeBefore(own, theirs, smoke) && abs(theirs - own) > 0.25 * own)
            {
                edge = true;
                continue;
            }
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
    // moving camera has left behind, is pulled back. By the edge of something standing before the smoke, though,
    // and where something stands before it where it was last frame, last frame's there may be that thing rather
    // than this smoke: kept within the colours round it, and counted for less, so no rim of it shows.
    float there = smokeMarchedDepth(frame, push.textures.w, int2(then * float2(frame->viewport)));
    bool unsure = edge || (there < far * 0.75 && own >= far * 0.75);
    float4 slack = unsure ? 0.0 : (high - low) + 0.1;
    return lerp(fresh, clamp(kept, low - slack, high + slack), unsure ? 0.75 : 0.9);
}

// The marched smoke (push.texture, push.size across) laid over the scene:
// each pixel from the four half-size ones round it. Where what the scene
// has there stands in front of the smoke, weighted most to those whose
// scene lies as deep as its own, so the smoke's edge follows it sharply;
// where the scene lies behind the smoke, blended evenly, so its edges do
// not show through (smokeAlike). push.textures.x is the scene's depth, and
// push.textures.y how far into the smoke its light comes from.
[shader("fragment")]
float4 smokeCompositeFragment(SmokeOut input) : SV_Target0
{
    Frame* frame = push.frame;
    Texture2D marched = textures[NonUniformResourceIndex(push.texture)];
    Texture2D into = textures[NonUniformResourceIndex(push.textures.y)];
    int2 half = int2(push.size);
    float own = smokeSceneDepth(frame, push.textures.x, int2(input.position.xy));
    float2 at = input.uv * float2(half) - 0.5;
    int2 base = int2(floor(at));
    float2 f = at - float2(base);
    if (own < SMOKE_NEAR)
    {
        // Something held right before the eye, as a gun, which the march went on past: only the smoke between it
        // and the eye, thinned there as the march thins it, and as thick as the smoke round about, whose light
        // comes from about as far in as one over the smoke's thickness.
        float4 around = 0.0;
        float far = 0.0;
        for (uint k = 0; k < 4; k++)
        {
            int2 o = int2(k & 1, k >> 1);
            int2 c = clamp(base + o, int2(0, 0), half - 1);
            float2 w = lerp(1.0 - f, f, float2(o));
            around += marched.Load(int3(c, 0)) * (w.x * w.y);
            far += into.Load(int3(c / 2, 0)).r * (w.x * w.y);
        }
        float x = own / SMOKE_NEAR;
        float depth = SMOKE_NEAR * x * x * x * (1.0 - 0.5 * x) / max(far, 0.05);
        float alpha = min(1.0 - exp(-depth), 1.0 - around.a);
        return float4(around.rgb / max(1.0 - around.a, 1e-3) * alpha, alpha);
    }
    // How far in the smoke's light comes from round here: the farthest of the four, as one with something
    // standing before the smoke has none of its own.
    float smoke = 0.0;
    for (uint k = 0; k < 4; k++)
    {
        smoke = max(smoke, into.Load(int3(clamp(base + int2(k & 1, k >> 1), int2(0, 0), half - 1) / 2, 0)).r);
    }
    let depthAt = (int2 c) => smokeMarchedDepth(frame, push.textures.x, int2((float2(c) + 0.5) / float2(half) * float2(frame->viewport)));
    // Whether a half-size pixel's scene lies about as deep as this one's, or both lie behind the smoke.
    let same = (float theirs) => !smokeBefore(own, theirs, smoke) || abs(theirs - own) <= 0.25 * own;
    float4 total = 0.0;
    float weights = 0.0;
    bool found = false;
    for (uint k = 0; k < 4; k++)
    {
        int2 o = int2(k & 1, k >> 1);
        int2 c = clamp(base + o, int2(0, 0), half - 1);
        float theirs = depthAt(c);
        float2 w = lerp(1.0 - f, f, float2(o));
        float weight = w.x * w.y * smokeAlike(own, theirs, smoke);
        total += marched.Load(int3(c, 0)) * weight;
        weights += weight;
        found = found || same(theirs);
    }
    float4 laid = total / max(weights, 1e-6);
    if (!found)
    {
        // The edge of something thin before the smoke, as a pole's, that none of the four lies on: from those a
        // little farther round that lie as deep; failing those, with only as much of the smoke as could lie before
        // it, little where it stands well nearer than where the smoke's light comes from.
        float4 near = 0.0;
        float nears = 0.0;
        for (int y = -1; y <= 2; y++)
        {
            for (int x = -1; x <= 2; x++)
            {
                int2 c = clamp(base + int2(x, y), int2(0, 0), half - 1);
                if (same(depthAt(c)))
                {
                    near += marched.Load(int3(c, 0));
                    nears += 1.0;
                }
            }
        }
        float before = saturate(own / max(smoke, 1e-3));
        laid = nears > 0.0 ? near / nears : lerp(float4(0.0, 0.0, 0.0, 1.0), laid, before * before);
    }
    return float4(laid.rgb, 1.0 - laid.a);
}
