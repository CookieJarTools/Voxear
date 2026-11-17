// Trails on water (see src/water_trails.rs): a square of ripples round the camera, its texels holding the world's
// cells modulo its size, each how far the water there is raised, how fast it is rising, how much foam lies on it and
// how much of that is fresh. Each frame `waterTrailsCarryMain` clears what came into the square and carries the rest
// along the way the water flows; then `waterTrailsMain`, a step or a few, runs the waves on by the wave equation, lets
// them die away, and has each body that cuts the surface push the water out of where it now is and let it back where
// it was, and leave foam. `trailAt` is what the water's surfaces read of it.

#pragma once

#include "common.slang"

// How fast the ripples run, in metres a second (src/water_trails.rs's WAVE_SPEED): what moves faster leaves a V.
static const float TRAIL_WAVE_SPEED = 1.1;
// How long the waves take to die away to a third, in seconds; and the water's level, back to flat.
static const float TRAIL_WAVES_FADE = 2.2;
static const float TRAIL_SETTLES = 12.0;
// How long foam takes to thin to a third, and its freshness.
static const float TRAIL_FOAM_FADES = 2.6;
static const float TRAIL_FRESH_FADES = 0.6;
// The share of the square at each edge over which the waves die away, so none comes round from the far side.
static const float TRAIL_EDGE = 0.12;
// How far a body pushes the water down under its middle, as many times its radius: the water it stands in, which it
// pushes out of its way as it moves on, lets back where it was, and pushes out as it goes in and lets back as it comes
// out. So a moving body is a hollow ahead of a hump, and what spreads from it is waves, never a trench left behind.
static const float TRAIL_DISPLACES = 1.0;
// How much a swimmer's strokes and treading lift and drop the water round it, a share of what it pushes away, and how
// many times a second; and as much as this of it for anything moving fast, pitching as it goes, so its V holds
// chevrons of waves.
static const float TRAIL_TREADING = 0.35;
static const float TRAIL_TREADS = 0.85;
static const float TRAIL_CHOP = 0.45;
// How much foam a body churns up a second, wholly in it, moving fast or plunging; and the waves, where they stand
// steeper than TRAIL_STEEP.
static const float TRAIL_FOAMING = 1.3;
// How much foam bursts up where a body plunges in or bursts out fast, for all of it crossing the surface.
static const float TRAIL_SPLASHING = 1.6;
static const float TRAIL_STEEP = 0.4;

// The world's cell slot `slot` of the square holds, its corner at `origin`.
int2 trailCell(Frame* frame, int2 slot, int2 origin)
{
    int n = int(frame->trails.texels);
    return origin + ((slot - origin) % n + n) % n;
}

// The slot of the square that holds cell `cell`.
int2 trailSlot(Frame* frame, int2 cell)
{
    int n = int(frame->trails.texels);
    return (cell % n + n) % n;
}

// How the water flows at `xz`, in metres a second, from the grid round the viewer the engine keeps; none off it.
float2 trailFlowAt(Frame* frame, float2 xz)
{
    float cell = frame->trails.flowCell;
    if (frame->trails.flowing == 0 || cell <= 0.0)
    {
        return 0.0;
    }
    float side = float(TRAIL_FLOW_CELLS);
    float2 at = (xz - frame->trails.flowCorner) / cell - 0.5;
    if (any(at < -0.5) || any(at > side - 0.5))
    {
        return 0.0;
    }
    int2 low = int2(floor(at));
    float2 f = at - float2(low);
    float2 flow = 0.0;
    for (int j = 0; j < 2; j++)
    {
        for (int i = 0; i < 2; i++)
        {
            int2 c = clamp(low + int2(i, j), 0, int(TRAIL_FLOW_CELLS) - 1);
            uint index = uint(c.y) * TRAIL_FLOW_CELLS + uint(c.x);
            float4 pair = frame->trails.flow[index / 2];
            float share = (i == 0 ? 1.0 - f.x : f.x) * (j == 0 ? 1.0 - f.y : f.y);
            flow += ((index & 1) == 0 ? pair.xy : pair.zw) * share;
        }
    }
    return flow;
}

// What the square held last frame at `cell`, in storage image `from`: nothing where the cell was not in it.
float4 trailKept(Frame* frame, uint from, int2 cell)
{
    int2 before = cell - frame->trails.lastOrigin;
    if (any(before < 0) || any(before >= int(frame->trails.texels)))
    {
        return 0.0;
    }
    return storageImagesRgba16f[from][trailSlot(frame, cell)];
}

// A Catmull-Rom spline's weights for the four texels round a place `t` of the way from the second to the third.
float4 trailSpline(float t)
{
    float t2 = t * t;
    float t3 = t2 * t;
    return float4(-0.5 * t3 + t2 - 0.5 * t, 1.5 * t3 - 2.5 * t2 + 1.0, -1.5 * t3 + 2.0 * t2 + 0.5 * t, 0.5 * t3 - 0.5 * t2);
}

// One texel of the square, from last frame's (storage image push.textures.x) into push.output: nothing where it has
// only now come into the square, or the square is new; else what was upstream of it, as far as the water flowed over
// the frame, read along a spline so ripples carried a long way down a river stay ripples.
[shader("compute")]
[numthreads(8, 8, 1)]
void waterTrailsCarryMain(uint3 id : SV_DispatchThreadID)
{
    Frame* frame = push.frame;
    uint n = frame->trails.texels;
    if (any(id.xy >= n))
    {
        return;
    }
    int2 slot = int2(id.xy);
    int2 cell = trailCell(frame, slot, frame->trails.origin);
    uint from = push.textures.x;
    float4 kept = 0.0;
    if (frame->trails.fresh == 0)
    {
        float texel = frame->trails.texel;
        float2 back = trailFlowAt(frame, (float2(cell) + 0.5) * texel) * frame->trails.step * float(frame->trails.steps) / texel;
        if (dot(back, back) < 1e-6)
        {
            kept = trailKept(frame, from, cell);
        }
        else
        {
            float2 source = float2(cell) - back;
            int2 base = int2(floor(source));
            float2 f = source - float2(base);
            float4 wx = trailSpline(f.x);
            float4 wz = trailSpline(f.y);
            for (int j = 0; j < 4; j++)
            {
                for (int i = 0; i < 4; i++)
                {
                    kept += trailKept(frame, from, base + int2(i - 1, j - 1)) * (wx[i] * wz[j]);
                }
            }
            kept.zw = saturate(kept.zw);
        }
    }
    storageImagesRgba16f[push.output][slot] = kept;
}

// How far body `stir`, the `index`th, pushes the water down at `centre`, `t` of the way through the frame and at the
// trails' clock `clock`: as far as its displacement there, by how much of it cuts the surface, rising and falling with
// its strokes or its pitching.
float trailDisplaced(TrailStir stir, uint index, float2 centre, float t, float clock)
{
    float2 a = lerp(stir.from.xy, stir.to.xy, t);
    float2 b = lerp(stir.from.zw, stir.to.zw, t);
    float2 ab = b - a;
    float length2 = dot(ab, ab);
    float along = length2 > 1e-8 ? saturate(dot(centre - a, ab) / length2) : 0.0;
    float r = distance(centre, a + ab * along) / stir.size.x;
    if (r >= 1.0)
    {
        return 0.0;
    }
    float under = 1.0 - r * r;
    float cut = lerp(stir.motion.x, stir.motion.y, t);
    float swaying = max(stir.size.w * TRAIL_TREADING, TRAIL_CHOP * saturate(stir.motion.w / 4.0));
    float sway = swaying * sin(6.2831853 * TRAIL_TREADS * clock + float(index) * 2.4);
    return TRAIL_DISPLACES * stir.size.x * stir.size.y * cut * under * under * (1.0 + sway);
}

// One step of the waves at a texel of the square, push.level of this frame's, from storage image push.textures.x into
// push.output. The wave equation over the texel and the eight round it (as evenly every way as nine texels allow),
// the waves dying away, fast near the square's edges; the water pushed down by as much more as each body that cuts the
// surface displaces here than it did a step ago, which is the waves it makes moving, going in or coming out, and a
// swimmer's treading; foam churned up there and where the waves stand steep, spreading a little and thinning away.
[shader("compute")]
[numthreads(8, 8, 1)]
void waterTrailsMain(uint3 id : SV_DispatchThreadID)
{
    Frame* frame = push.frame;
    int n = int(frame->trails.texels);
    if (any(id.xy >= uint(n)))
    {
        return;
    }
    uint from = push.textures.x;
    int2 slot = int2(id.xy);
    float4 here = storageImagesRgba16f[from][slot];
    float4 east = storageImagesRgba16f[from][(slot + int2(1, 0)) % n];
    float4 west = storageImagesRgba16f[from][(slot + int2(n - 1, 0)) % n];
    float4 north = storageImagesRgba16f[from][(slot + int2(0, 1)) % n];
    float4 south = storageImagesRgba16f[from][(slot + int2(0, n - 1)) % n];
    float corners = storageImagesRgba16f[from][(slot + int2(1, 1)) % n].x + storageImagesRgba16f[from][(slot + int2(n - 1, 1)) % n].x
        + storageImagesRgba16f[from][(slot + int2(1, n - 1)) % n].x + storageImagesRgba16f[from][(slot + int2(n - 1, n - 1)) % n].x;
    float dt = frame->trails.step;
    float texel = frame->trails.texel;
    float sides = east.x + west.x + north.x + south.x;
    float bent = (4.0 * sides + corners - 20.0 * here.x) / (6.0 * texel * texel);
    float rising = here.y + TRAIL_WAVE_SPEED * TRAIL_WAVE_SPEED * bent * dt;

    // Near the square's edges the waves die away fast.
    int2 cell = trailCell(frame, slot, frame->trails.origin);
    int2 within = cell - frame->trails.origin;
    float edge = float(min(min(within.x, within.y), min(n - 1 - within.x, n - 1 - within.y))) / (float(n) * TRAIL_EDGE);
    float kept = smoothstep(0.0, 1.0, saturate(edge));
    rising *= exp(-dt / (TRAIL_WAVES_FADE * lerp(0.03, 1.0, kept)));

    // Stirred, by each body where it is this step, from where it was last frame to where it is now (where the water
    // that was under it then has flowed to, so a body carried along with the water stirs none of it).
    float2 centre = (float2(cell) + 0.5) * texel;
    float steps = float(max(frame->trails.steps, 1u));
    float t0 = float(push.level) / steps;
    float t1 = (float(push.level) + 1.0) / steps;
    float clock = frame->trails.clock - dt * (steps - float(push.level) - 1.0);
    float pushed = 0.0;
    float foam = here.z;
    float fresh = here.w;
    for (uint i = 0; i < frame->trails.count; i++)
    {
        TrailStir stir = frame->trails.stirs[i];
        pushed += trailDisplaced(stir, i, centre, t1, clock) - trailDisplaced(stir, i, centre, t0, clock - dt);
        // Foam over all the width it cuts, the more the faster it moves and the harder it plunges.
        float2 a = lerp(stir.from.xy, stir.to.xy, t1);
        float2 b = lerp(stir.from.zw, stir.to.zw, t1);
        float2 ab = b - a;
        float length2 = dot(ab, ab);
        float along = length2 > 1e-8 ? saturate(dot(centre - a, ab) / length2) : 0.0;
        float r = distance(centre, a + ab * along) / stir.size.x;
        if (r < 1.0)
        {
            float cutting = lerp(stir.motion.x, stir.motion.y, t1);
            float churned = 0.7 * smoothstep(0.4, 2.5, stir.motion.w) + 1.5 * smoothstep(0.8, 3.0, abs(stir.motion.z));
            float made = TRAIL_FOAMING * churned * cutting * (1.0 - r * r) * stir.size.z * dt;
            // What plunges in or bursts out crosses the surface in a moment: a burst of foam as much as it crossed this
            // step, the more the faster it went.
            float crossed = abs(stir.motion.y - stir.motion.x) / steps;
            made += TRAIL_SPLASHING * crossed * smoothstep(1.0, 5.0, abs(stir.motion.z)) * (1.0 - r * r) * stir.size.z;
            foam += made;
            fresh += made;
        }
    }
    float height = (here.x + rising * dt - pushed) * exp(-dt / (TRAIL_SETTLES * lerp(0.01, 1.0, kept)));

    // Foam where the waves stand steep, as a fast body's bow wave breaks; spreading a little, and thinning away.
    float2 slope = float2(east.x - west.x, north.x - south.x) / (2.0 * texel);
    float steep = saturate(length(slope) / TRAIL_STEEP - 1.0);
    foam += steep * 1.2 * dt;
    fresh += steep * 0.8 * dt;
    float spread = 1.0 - exp(-dt * 0.25);
    foam = lerp(foam, (east.z + west.z + north.z + south.z) * 0.25, spread) * exp(-dt / TRAIL_FOAM_FADES);
    fresh = lerp(fresh, (east.w + west.w + north.w + south.w) * 0.25, spread) * exp(-dt / TRAIL_FRESH_FADES);
    foam = foam < 2e-3 ? 0.0 : saturate(foam) * kept;
    storageImagesRgba16f[push.output][slot] = float4(height, rising, foam, min(saturate(fresh), foam));
}

// How the trails look (src/water_trails.rs's TrailLook): blocky on worlds of blocks and smooth on other water, smooth
// everywhere, or blocky everywhere.
static const uint TRAIL_MATCH = 0;
static const uint TRAIL_SMOOTH = 1;
static const uint TRAIL_BLOCKY = 2;

// What the trails lay over the water at a place.
struct Trail
{
    // The slopes of their ripples along x and z; the share of them too fine for a pixel, as a variance of slopes, for
    // roughness instead.
    float2 slope;
    float variance;
    // How much foam lies there, and how much of it is fresh, 0 to 1.
    float foam;
    float fresh;
    // In the blocky look, how wide its squares are, in metres (0 when smooth); where a square stands a step above
    // the one beside it, its lip, lit, more than 0, and where it stands below, the step's shadow, less than 0; and
    // how many steps the square stands up or down, from -1 to 1 over three, for raised squares to catch more light.
    float square;
    float lip;
    float raised;
};

// How wide a pixel is on the water at `xz`, in metres: what sharpens or blurs what is read of the trails there.
float trailFootprint(float2 xz)
{
    return max(length(ddx(xz)), length(ddy(xz)));
}

// How wide the blocky look's squares are on water whose blocks are `blockSize` across (0 for water not of blocks), in
// metres: a quarter of a block, as a rough sea's small cubes; 0 where the trails look smooth.
float trailSquare(Frame* frame, float blockSize)
{
    uint look = frame->trails.look;
    bool squares = look == TRAIL_BLOCKY || (look == TRAIL_MATCH && blockSize > 0.0);
    return squares ? max((blockSize > 0.0 ? blockSize : 1.0) * 0.25, frame->trails.texel) : 0.0;
}

// The trails at `xz`, a pixel `footprint` metres across there, on water whose blocks are `blockSize` across (0 for
// water not of blocks): none off the square round the camera, fading out towards its edges, and ripples too fine for
// a pixel as roughness. Blocky, the water stands in small squares, each flat across, raised or lowered a whole step at
// a time by the ripples under it and tilted as they lean there, so the trails read as cubes as the water's blocks do.
Trail trailAt(Frame* frame, float2 xz, float footprint, float blockSize)
{
    Trail t;
    t.slope = 0.0;
    t.variance = 0.0;
    t.foam = 0.0;
    t.fresh = 0.0;
    t.square = 0.0;
    t.lip = 0.0;
    t.raised = 0.0;
    uint texels = frame->trails.texels;
    if (texels == 0 || frame->trails.texture == NO_TEXTURE)
    {
        return t;
    }
    float texel = frame->trails.texel;
    float span = float(texels) * texel;
    float2 middle = (float2(frame->trails.origin) + float(texels) * 0.5) * texel;
    float2 off = abs(xz - middle) / (span * 0.5);
    float inside = 1.0 - smoothstep(1.0 - 2.5 * TRAIL_EDGE, 1.0 - TRAIL_EDGE, max(off.x, off.y));
    if (inside <= 0.0)
    {
        return t;
    }
    float square = trailSquare(frame, blockSize);
    float apart = square > 0.0 ? square : texel;
    float2 at = square > 0.0 ? (floor(xz / apart) + 0.5) * apart : xz;
    // The square repeats as its texels hold the world's cells, so the world's place, wrapped, is where it is.
    Texture2D field = textures[NonUniformResourceIndex(frame->trails.texture)];
    SamplerState wrapped = samplers[SAMPLER_LINEAR_REPEAT];
    float2 uv = at / span;
    float d = apart / span;
    float4 here = field.SampleLevel(wrapped, uv, 0.0);
    float east = field.SampleLevel(wrapped, uv + float2(d, 0.0), 0.0).x;
    float west = field.SampleLevel(wrapped, uv - float2(d, 0.0), 0.0).x;
    float north = field.SampleLevel(wrapped, uv + float2(0.0, d), 0.0).x;
    float south = field.SampleLevel(wrapped, uv - float2(0.0, d), 0.0).x;
    float2 slope = float2(east - west, north - south) / (2.0 * apart);
    // Sharp while a pixel spans no more than two texels, then into the roughness by four.
    float seen = saturate(2.0 - 0.5 * footprint / (square > 0.0 ? apart * 0.5 : texel));
    t.slope = slope * inside * seen;
    t.variance = dot(slope, slope) * inside * (1.0 - seen);
    t.foam = saturate(here.z) * inside;
    t.fresh = saturate(here.w) * inside;
    if (square > 0.0)
    {
        // Each square a whole number of steps up or down, a quarter of its width each; at its edges, how many steps it
        // stands over the square beside it there, as a lip lit on its top, or under it, in the shadow of the step.
        float rise = apart * 0.25;
        float level = round(here.x / rise);
        float2 within = frac(xz / apart);
        float edge = 0.16;
        float steps = 0.0;
        steps += within.x > 1.0 - edge ? level - round(east / rise) : 0.0;
        steps += within.x < edge ? level - round(west / rise) : 0.0;
        steps += within.y > 1.0 - edge ? level - round(north / rise) : 0.0;
        steps += within.y < edge ? level - round(south / rise) : 0.0;
        t.square = apart;
        t.lip = clamp(steps, -2.0, 2.0) * 0.5 * inside * seen;
        t.raised = clamp(level / 3.0, -1.0, 1.0) * inside * seen;
    }
    return t;
}

// A number from 0 to 1 for a whole-numbered square, the same wherever it is worked out.
float trailGrain(int2 cell)
{
    uint h = uint(cell.x) * 73856093u ^ uint(cell.y) * 19349663u;
    h ^= h >> 13;
    h *= 0x5bd1e995u;
    h ^= h >> 15;
    return float(h & 0xFFFFu) / 65535.0;
}

// The blocky look's foam at `xz` for trail `t`: whole squares white, more of them the more foam there is, each from a
// number of its own, so it thins square by square. None where the trails look smooth.
float trailSquares(Trail t, float2 xz)
{
    if (t.square <= 0.0 || t.foam <= 0.0)
    {
        return 0.0;
    }
    int2 square = int2(floor(xz / t.square));
    return step(trailGrain(square), saturate(t.foam * 1.6 - 0.1)) * saturate(t.foam * 6.0);
}
