// Smoke (src/smoke.rs): clouds filling the space round their Objs, each a
// grid of cells the engine filled out from its middle against the level's
// colliders. Marched at half the view's size, lit by the sun through the
// cloud and by the light round it, then laid over the scene at full size,
// each pixel from the half-size ones nearest it in depth.

#include "particle_shared.slang"

// How many cells a smoke's grid has along each side (smoke::SMOKE_GRID).
static const uint SMOKE_GRID = 32;
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

// Smooth noise in 3D, from 0 to 1.
float smokeNoise(float3 p)
{
    float3 i = floor(p);
    float3 f = frac(p);
    f = f * f * (3.0 - 2.0 * f);
    let corner = (float3 o) => frac(sin(dot(i + o, float3(127.1, 311.7, 74.7))) * 43758.5453);
    float x00 = lerp(corner(float3(0, 0, 0)), corner(float3(1, 0, 0)), f.x);
    float x10 = lerp(corner(float3(0, 1, 0)), corner(float3(1, 1, 0)), f.x);
    float x01 = lerp(corner(float3(0, 0, 1)), corner(float3(1, 0, 1)), f.x);
    float x11 = lerp(corner(float3(0, 1, 1)), corner(float3(1, 1, 1)), f.x);
    return lerp(lerp(x00, x10, f.y), lerp(x01, x11, f.y), f.z);
}

// How a shot's and a blast's holes open and fill back in (smoke::shot_open
// and smoke::blast_open).
static const float SHOT_RADIUS = 0.5;
static const float2 SHOT_REFILL = float2(0.4, 1.6);
static const float2 BLAST_REFILL = float2(0.5, 5.0);
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
        open -= shell * (1.0 - smoothstep(0.0, BLAST_REFILL.y * 0.6, age)) * BLAST_PUSH;
        left *= age >= 0.0 ? 1.0 - open : 1.0;
    }
    return left;
}

// How thick `smoke` is at `world`, as light a metre takes: its cells blended
// smoothly, and with `swirls`, eaten into by drifting noise, most at its
// edges, so it billows rather than showing its cells.
float smokeDensity(Smoke smoke, float3 world, bool swirls)
{
    // Read a little off where it is, by drifting noise, so the cloud's edge does not follow its cells' rows.
    if (swirls)
    {
        float3 w = world * 1.1 + smoke.seed * 0.37 + float3(0.0, -smoke.age * 0.2, 0.0);
        float3 warp = float3(smokeNoise(w), smokeNoise(w + 19.1), smokeNoise(w + 41.7)) - 0.5;
        world += warp * smoke.cell * 1.6;
    }
    float3 g = (world - smoke.origin) / smoke.cell - 0.5;
    int3 c = int3(floor(g));
    float3 f = g - float3(c);
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
        return filled * smoke.density * smokeLeft(smoke, world, 0.0);
    }
    float3 q = world * 0.45 + smoke.seed + float3(0.0, -smoke.age * 0.12, 0.0);
    float n = smokeNoise(q) * 0.55 + smokeNoise(q * 2.3 + 5.3) * 0.3 + smokeNoise(q * 5.1 + 2.1) * 0.15;
    // The holes' edges ragged by the same swirls.
    // Thicker and thinner through and through, not only at its edge, so from inside it swirls past.
    float swirled = saturate(filled * 1.7 - (1.0 - n) * 1.1) * (0.3 + 1.4 * n * n);
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
        if (all(local >= 0.0) && all(local <= 1.0))
        {
            total += smokeDensity(smoke, world, swirls);
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

// The smokes along this pixel's view, as far as the scene behind: the light
// they send towards the eye, fogged as far off as they start, and in alpha
// how much of what lies behind shows through them. push.textures: x the
// scene's depth, y the fog's volume, z how many smokes.
[shader("fragment")]
SmokeMarched smokeMarchFragment(SmokeOut input)
{
    SmokeMarched none = { float4(0.0, 0.0, 0.0, 1.0), 0.0 };
    Frame* frame = push.frame;
    uint count = push.textures.z;
    float4 nearH = transform(frame->invViewProj, float4(input.uv * 2.0 - 1.0, 1.0, 1.0));
    float3 eye = frame->cameraPosition;
    float3 way = normalize(nearH.xyz / nearH.w - eye);
    float behind = textures[NonUniformResourceIndex(push.textures.x)].Load(int3(int2(input.uv * float2(frame->viewport)), 0)).r;
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
    enter = max(enter, 0.0);
    leave = min(leave, scene);
    if (enter >= leave)
    {
        return none;
    }
    float step = max(cell * 0.5, (leave - enter) / float(SMOKE_STEPS));
    float jitter = frac(52.9829189 * frac(dot(input.position.xy, float2(0.06711056, 0.00583715)) + float(frame->frameIndex % 64) * 0.61803));

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
    float first = -1.0;
    float weighed = 0.0;
    float weights = 0.0;
    for (uint i = 0; i < SMOKE_STEPS; i++)
    {
        float t = enter + (float(i) + jitter) * step;
        if (t > leave || through < 0.01)
        {
            break;
        }
        float3 at = eye + way * t;
        // Thinner right by the eye, so from inside the view does not begin at a wall of it.
        float thick = smokesAt(frame, count, at, true) * smoothstep(0.0, 0.8, t);
        if (thick < 1e-3)
        {
            continue;
        }
        first = first < 0.0 ? t : first;
        // The sun's light through the cloud between it and here, and past what shadows it.
        float toward = 0.0;
        for (uint k = 1; k <= SMOKE_SUN_STEPS; k++)
        {
            toward += smokesAt(frame, count, at + toSun * cell * 1.5 * float(k), false);
        }
        // Looked up a little off where it is, differently each frame and step, so the shadows of what stands in
        // the sun soften over frames rather than stepping with the shadow map's texels.
        float3 shake = float3(frac(jitter * 7.13 + float(i) * 0.618), frac(jitter * 3.71 + float(i) * 0.382), frac(jitter * 5.37)) - 0.5;
        float sun = exp(-toward * cell * 1.5) * sunShadowHard(frame, at + shake * cell, toSun);
        // The lamps in this pixel's cluster, scattered as evenly every way.
        float3 lamps = 0.0;
        uint base = clusterAt(frame, input.position.xy * 2.0, t * dot(way, frame->cameraForward)) * (frame->clusterMaxLights + 1);
        for (uint l = 0; l < buffers()->clusters[base]; l++)
        {
            Light lamp = buffers()->lights[buffers()->clusters[base + 1 + l]];
            lamps += localLightInput(lamp, at, normalizeOr(lamp.position - at, way)).color;
        }
        float3 lit = color * (sunLight * sun * phase * PI + lamps * 0.25 + ambient);
        float kept = exp(-thick * step);
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
    Texture2D current = textures[NonUniformResourceIndex(push.texture)];
    int2 pixel = int2(input.position.xy);
    int2 size = int2(push.size);
    float4 now = current.Load(int3(pixel, 0));
    float far = textures[NonUniformResourceIndex(push.textures.x)].Load(int3(pixel, 0)).r;
    if (push.textures.z == 0 || far <= 0.0)
    {
        return now;
    }
    float4 low = now;
    float4 high = now;
    for (int y = -1; y <= 1; y++)
    {
        for (int x = -1; x <= 1; x++)
        {
            float4 round = current.Load(int3(clamp(pixel + int2(x, y), int2(0, 0), size - 1), 0));
            low = min(low, round);
            high = max(high, round);
        }
    }
    float4 nearH = transform(frame->invViewProj, float4(input.uv * 2.0 - 1.0, 1.0, 1.0));
    float3 way = normalize(nearH.xyz / nearH.w - frame->cameraPosition);
    float4 before = transform(frame->prevViewProj, float4(frame->cameraPosition + way * far, 1.0));
    float2 then = before.xy / before.w * 0.5 + 0.5;
    if (before.w <= 0.0 || any(then < 0.0) || any(then > 1.0))
    {
        return now;
    }
    float4 kept = textures[NonUniformResourceIndex(push.textures.y)].SampleLevel(samplers[SAMPLER_LINEAR_CLAMP], then, 0.0);
    return lerp(now, clamp(kept, low, high), 0.85);
}

// The scene's depth along the view at full size, in metres; far for the sky.
float smokeSceneDepth(Frame* frame, int2 pixel)
{
    float depth = textures[NonUniformResourceIndex(push.textures.x)].Load(int3(pixel, 0)).r;
    return depth > 0.0 ? frame->near / depth : 1e4;
}

// The marched smoke (push.texture, push.size across) laid over the scene:
// each pixel from the four half-size ones round it, weighted most to those
// whose scene lies as deep as its own, so the smoke's edge follows what
// stands in front of it sharply.
[shader("fragment")]
float4 smokeCompositeFragment(SmokeOut input) : SV_Target0
{
    Frame* frame = push.frame;
    Texture2D marched = textures[NonUniformResourceIndex(push.texture)];
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
        float weight = w.x * w.y / (1e-3 + abs(theirs - own) / max(own, 1e-3));
        total += marched.Load(int3(c, 0)) * weight;
        weights += weight;
    }
    float4 smoke = total / max(weights, 1e-6);
    return float4(smoke.rgb, 1.0 - smoke.a);
}
