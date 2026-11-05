// The sea (src/ocean.rs, src/water.rs). Its waves are an FFT of an ocean
// spectrum, as Sea of Thieves makes its sea: four cascades, each moved on
// in time and summed by an inverse FFT along its rows then its columns
// (oceanRowsMain, oceanColumnsMain), into how far the water at each point
// is moved, how much it folds at a crest, and its slopes, with mips
// (oceanMipMain). The surface is a grid of rings round the camera, each
// twice as coarse as the one inside it (waterVertex), lit as Sea of Thieves
// and Atlas light theirs (waterFragment).

#include "glass.slang"

// The cascades' resolution and how many there are (OCEAN_CASCADES in src/ocean.rs).
static const uint OCEAN_N = 256;
static const uint OCEAN_CASCADES = 4;
// The sea's grid: rings of this many quads across, the first this many metres apart, each twice the last.
static const uint WATER_QUADS = 64;
static const uint WATER_RINGS = 12;
static const float WATER_SPACING = 0.25;
// A Pool's grid: this many quads along each side.
static const uint POOL_QUADS = 64;
// A Stream's strip: this many quads across it at each point.
static const uint STREAM_ACROSS = 8;
// How long a Stream's ripples drift with it before they start again (two-phase flow).
static const float FLOW_PERIOD = 1.5;
// About how long a lava lake's surface lasts before it turns over, in seconds.
static const float LAKE_TURNOVER = 15.0;
// What a WaterOut is the sea's rather than a Pool's.
static const uint SEA = 0xFFFFFFFF;
static const float GRAVITY = 9.81;
// The sea repeats after this many seconds (REPEAT_SECONDS in src/ocean.rs).
static const float OCEAN_REPEAT = 1000.0;

// ---------------------------------------------------------------- The FFT

// A row or a column of four fields at once, each two real fields packed as
// one complex one: x + i y, z + i ∂y/∂x, ∂y/∂z + i ∂x/∂x, ∂z/∂z + i ∂x/∂z.
groupshared float2 oceanRow[4][OCEAN_N];

float2 complexMul(float2 a, float2 b)
{
    return float2(a.x * b.x - a.y * b.y, a.x * b.y + a.y * b.x);
}

// The inverse FFT of the four fields in oceanRow, in place, its values
// stored in bit-reversed order to begin with: 128 threads, each a butterfly
// of each field a stage.
void inverseFft(uint thread)
{
    for (uint half = 1; half < OCEAN_N; half <<= 1)
    {
        GroupMemoryBarrierWithGroupSync();
        uint j = thread % half;
        uint first = (thread / half) * half * 2 + j;
        float angle = 3.14159265 * float(j) / float(half);
        float2 w = float2(cos(angle), sin(angle));
        for (uint f = 0; f < 4; f++)
        {
            float2 a = oceanRow[f][first];
            float2 b = complexMul(oceanRow[f][first + half], w);
            oceanRow[f][first] = a + b;
            oceanRow[f][first + half] = a - b;
        }
    }
    GroupMemoryBarrierWithGroupSync();
}

uint reversed(uint i)
{
    return reversebits(i) >> 24;
}

// A frequency's place, -N/2 to N/2, from its index.
int frequencyOf(uint i)
{
    return i < OCEAN_N / 2 ? int(i) : int(i) - int(OCEAN_N);
}

// A wave number's frequency in deep water, rounded to a whole number of
// turns in OCEAN_REPEAT, as the CPU's (ocean::frequency).
float waveFrequency(float k)
{
    float step = 6.2831853 / OCEAN_REPEAT;
    return floor(sqrt(GRAVITY * k) / step) * step;
}

// Row push.list of cascade push.level: each wave moved on to now, as its
// four fields, summed along the row. Its spectrum at buffers()->oceanSpectrum,
// h0(k) and h0*(-k) for each wave; the sums to buffers()->oceanFft.
[shader("compute")]
[numthreads(128, 1, 1)]
void oceanRowsMain(uint3 group : SV_GroupID, uint3 local : SV_GroupThreadID)
{
    Frame* frame = push.frame;
    uint cascade = push.level;
    uint row = group.x;
    float size = frame->ocean.sizes[cascade];
    float dk = 6.2831853 / size;
    float time = frame->ocean.time;
    for (uint e = 0; e < 2; e++)
    {
        uint column = local.x + e * 128;
        float4 waves = buffers()->oceanSpectrum[(cascade * OCEAN_N + row) * OCEAN_N + column];
        float2 k = float2(frequencyOf(column), frequencyOf(row)) * dk;
        float length = sqrt(dot(k, k));
        float4 fields[2] = { float4(0.0), float4(0.0) };
        if (length > 1e-6)
        {
            float phase = waveFrequency(length) * time;
            float2 turn = float2(cos(phase), sin(phase));
            float2 h = complexMul(waves.xy, turn) + complexMul(waves.zw, float2(turn.x, -turn.y));
            float2 ih = float2(-h.y, h.x);
            // Water moved towards each crest, i k/|k| h; the slopes, i k h; and how the sideways movement changes.
            float2 dx = ih * (k.x / length);
            float2 dz = ih * (k.y / length);
            float2 dydx = ih * k.x;
            float2 dydz = ih * k.y;
            float2 dxdx = -h * (k.x * k.x / length);
            float2 dzdz = -h * (k.y * k.y / length);
            float2 dxdz = -h * (k.x * k.y / length);
            // a + i b, both real fields: a's spectrum plus i times b's.
            let pack = (float2 a, float2 b) => float2(a.x - b.y, a.y + b.x);
            fields[0] = float4(pack(dx, h), pack(dz, dydx));
            fields[1] = float4(pack(dydz, dxdx), pack(dzdz, dxdz));
        }
        uint at = reversed(column);
        oceanRow[0][at] = fields[0].xy;
        oceanRow[1][at] = fields[0].zw;
        oceanRow[2][at] = fields[1].xy;
        oceanRow[3][at] = fields[1].zw;
    }
    inverseFft(local.x);
    for (uint e = 0; e < 2; e++)
    {
        uint x = local.x + e * 128;
        for (uint f = 0; f < 4; f++)
        {
            buffers()->oceanFft[((cascade * 4 + f) * OCEAN_N + row) * OCEAN_N + x] = oceanRow[f][x];
        }
    }
}

// Column push.list of cascade push.level: the rows' sums summed down the
// column, then each point's movement, how much the water folds there (the
// Jacobian, under 1 where a crest breaks), and its slopes and their
// squares, for roughness from the mips: into storage images push.output
// and push.textures.x.
[shader("compute")]
[numthreads(128, 1, 1)]
void oceanColumnsMain(uint3 group : SV_GroupID, uint3 local : SV_GroupThreadID)
{
    Frame* frame = push.frame;
    uint cascade = push.level;
    uint column = group.x;
    for (uint e = 0; e < 2; e++)
    {
        uint row = local.x + e * 128;
        for (uint f = 0; f < 4; f++)
        {
            oceanRow[f][reversed(row)] = buffers()->oceanFft[((cascade * 4 + f) * OCEAN_N + row) * OCEAN_N + column];
        }
    }
    inverseFft(local.x);
    float chop = frame->ocean.choppiness;
    for (uint e = 0; e < 2; e++)
    {
        uint z = local.x + e * 128;
        float2 a = oceanRow[0][z];
        float2 b = oceanRow[1][z];
        float2 c = oceanRow[2][z];
        float2 d = oceanRow[3][z];
        float3 moved = float3(a.x * chop, a.y, b.x * chop);
        float dxdx = c.y * chop;
        float dzdz = d.x * chop;
        float dxdz = d.y * chop;
        float folded = (1.0 + dxdx) * (1.0 + dzdz) - dxdz * dxdz;
        float2 slope = float2(b.y / max(1.0 + dxdx, 0.1), c.x / max(1.0 + dzdz, 0.1));
        storageImagesRgba16f[push.output][uint2(column, z)] = float4(moved, folded);
        storageImagesRgba16f[push.textures.x][uint2(column, z)] = float4(slope, slope * slope);
    }
}

// Mip push.level + 1 of a cascade's image, push.texture, from mip
// push.level: each texel the average of the four under it.
[shader("compute")]
[numthreads(8, 8, 1)]
void oceanMipMain(uint3 id : SV_DispatchThreadID)
{
    if (any(id.xy >= push.size))
    {
        return;
    }
    Texture2D source = textures[NonUniformResourceIndex(push.texture)];
    int2 at = int2(id.xy) * 2;
    float4 sum = source.Load(int3(at, push.level)) + source.Load(int3(at + int2(1, 0), push.level))
        + source.Load(int3(at + int2(0, 1), push.level)) + source.Load(int3(at + int2(1, 1), push.level));
    storageImagesRgba16f[push.output][id.xy] = sum * 0.25;
}

// The foam round the camera: a square of FOAM_TEXELS a side, FOAM_TEXEL
// metres each, kept from frame to frame (as Crest's and Atlas's).
static const uint FOAM_TEXELS = 512;
static const float FOAM_TEXEL = 0.5;

// How much foam a crest breaking at `rest` leaves behind: only where the
// water folds well past where crest foam begins, so the big breakers trail
// foam and the small ones' white is gone as they pass.
float breakingAt(Frame* frame, float2 rest)
{
    float folded = 1.0;
    for (uint c = 0; c < OCEAN_CASCADES; c++)
    {
        float4 d = textures[NonUniformResourceIndex(frame->ocean.displacement[c])].SampleLevel(samplers[SAMPLER_LINEAR_REPEAT], cascadeUv(frame, c, rest), 1.0);
        folded += d.w - 1.0;
    }
    float threshold = 0.1 + 0.45 * frame->ocean.foam;
    return frame->ocean.foam > 0.0 ? saturate((threshold - folded) / 0.3) : 0.0;
}

// One texel of the foam round the camera (storage image push.output): the
// foam it had a frame ago, fading over a few seconds, and new foam where
// crests break now. It is kept by where the water rests, as the surface is
// drawn, so it rides the waves' orbits as the water does and stays behind as
// a breaking crest runs on: a streak down the wave's back. Last frame's foam
// in push.textures.x, its first texel's corner in the bits of
// push.textures.yz, NaN on the first frame.
[shader("compute")]
[numthreads(8, 8, 1)]
void oceanFoamMain(uint3 id : SV_DispatchThreadID)
{
    if (any(id.xy >= FOAM_TEXELS))
    {
        return;
    }
    Frame* frame = push.frame;
    float2 here = frame->ocean.foamOrigin + (float2(id.xy) + 0.5) * FOAM_TEXEL;
    float dt = max(frame->deltaTime, 1e-4);
    float2 previousOrigin = float2(asfloat(push.textures.y), asfloat(push.textures.z));
    float kept = 0.0;
    if (!isnan(previousOrigin.x))
    {
        float2 uv = (here - previousOrigin) / (float(FOAM_TEXELS) * FOAM_TEXEL);
        if (all(uv > 0.0) && all(uv < 1.0))
        {
            kept = textures[NonUniformResourceIndex(push.textures.x)].SampleLevel(samplers[SAMPLER_LINEAR_CLAMP], uv, 0.0).r;
        }
    }
    // As much as a crest breaking here makes, fading over a few seconds once it has passed.
    float foam = max(kept * exp(-dt / 2.0), breakingAt(frame, here));
    storageImagesR32f[push.output][id.xy] = saturate(foam);
}

// The foam round the camera at `rest`: what crests left there, carried
// along by the water; none outside the square it keeps.
float keptFoam(Frame* frame, float2 rest)
{
    float2 uv = (rest - frame->ocean.foamOrigin) / (float(FOAM_TEXELS) * FOAM_TEXEL);
    if (any(uv <= 0.0) || any(uv >= 1.0))
    {
        return 0.0;
    }
    return textures[NonUniformResourceIndex(frame->ocean.foamTexture)].SampleLevel(samplers[SAMPLER_LINEAR_CLAMP], uv, 0.0).r;
}

// The scene's depth before the sea is drawn into it, push.texture, into the
// R32F storage image push.output: for how deep the water is to what lies
// under it.
[shader("compute")]
[numthreads(8, 8, 1)]
void waterDepthCopyMain(uint3 id : SV_DispatchThreadID)
{
    if (any(id.xy >= push.size))
    {
        return;
    }
    storageImagesR32f[push.output][id.xy] = textures[NonUniformResourceIndex(push.texture)].Load(int3(id.xy, 0)).r;
}

// ---------------------------------------------------------------- The surface

struct WaterOut
{
    float4 position : SV_Position;
    float3 world : POSITION;
    // Where the water drawn there rests, before the waves move it.
    float2 rest : REST;
    float4 currentClip : CURRENT_CLIP;
    float4 previousClip : PREVIOUS_CLIP;
    // SEA, or the Pool or Stream it is of.
    nointerpolation uint body : BODY;
    // A Stream's: how it flows across the ground, in metres a second; how much it foams; and how far it is from
    // either end, in points.
    float4 stream : STREAM;
    // Which way is up from the water's surface before its ripples: a Stream's as its bed slopes.
    float3 up : UP;
    // A Stream's: how long what flows there has taken to come from its start, in seconds; and how far across it,
    // from -1 at one bank to 1 at the other.
    float2 lava : LAVA;
};

float2 cascadeUv(Frame* frame, uint cascade, float2 rest)
{
    return rest / frame->ocean.sizes[cascade] + 0.5 / float(OCEAN_N);
}

// How much of cascade `cascade` water has whose `swell` says how much of
// the sea's long waves it takes: a Pool has the short waves and ripples
// always, so raised or lowered it moves as the sea round it does.
float cascadeShare(uint cascade, float swell)
{
    return cascade >= 2 ? 1.0 : swell;
}

// How far the waves move the water resting at `rest`, sampled as coarsely
// as the grid there is `spacing` metres apart, so waves shorter than it do
// not alias; last frame's with `previous`; with as much of the long swell
// as `swell` says.
float3 oceanMoved(Frame* frame, float2 rest, float spacing, bool previous, float swell = 1.0)
{
    float3 moved = 0.0;
    for (uint c = 0; c < OCEAN_CASCADES; c++)
    {
        uint image = previous ? frame->ocean.previous[c] : frame->ocean.displacement[c];
        float mip = max(log2(spacing / (frame->ocean.sizes[c] / float(OCEAN_N))), 0.0);
        float3 d = textures[NonUniformResourceIndex(image)].SampleLevel(samplers[SAMPLER_LINEAR_REPEAT], cascadeUv(frame, c, rest), mip).xyz;
        moved += d * cascadeShare(c, swell);
    }
    return moved;
}

// A corner of a quad of ring `ring`, round the camera. Each ring leaves out
// the quads the finer ring inside it covers; its rings meet at multiples of
// its own spacing, so a quad is wholly in or out. Near its outer edge its
// odd vertices slide onto the coarser ring's, so the two meet without cracks.
[shader("vertex")]
WaterOut waterVertex(uint vertexId : SV_VertexID, uint ring : SV_InstanceID)
{
    Frame* frame = push.frame;
    static const uint2 corners[6] = { uint2(0, 0), uint2(0, 1), uint2(1, 0), uint2(1, 0), uint2(0, 1), uint2(1, 1) };
    uint quad = vertexId / 6;
    uint2 cell = uint2(quad % WATER_QUADS, quad / WATER_QUADS);
    float spacing = WATER_SPACING * exp2(float(ring));
    float2 centre = floor(frame->cameraPosition.xz / (2.0 * spacing)) * 2.0 * spacing;
    float extent = float(WATER_QUADS) * 0.5 * spacing;
    WaterOut o;
    if (ring > 0)
    {
        float finer = spacing * 0.5;
        float2 finerCentre = floor(frame->cameraPosition.xz / (2.0 * finer)) * 2.0 * finer;
        float2 middle = centre - extent + (float2(cell) + 0.5) * spacing;
        if (all(abs(middle - finerCentre) < float(WATER_QUADS) * 0.5 * finer))
        {
            return nothing();
        }
    }
    uint2 grid = cell + corners[vertexId % 6];
    float2 rest = centre - extent + float2(grid) * spacing;
    float edge = float(min(min(grid.x, grid.y), min(WATER_QUADS - grid.x, WATER_QUADS - grid.y)));
    float morph = ring + 1 < WATER_RINGS ? saturate(1.0 - edge / 8.0) : 0.0;
    rest -= float2(grid % 2) * spacing * morph;
    // Sampled as coarsely as the coarser ring where they meet, so both put a shared vertex at one height.
    float sampled = spacing * exp2(morph);
    float level = frame->ocean.level;
    float3 world = float3(rest.x, level, rest.y) + oceanMoved(frame, rest, sampled, false);
    float3 before = float3(rest.x, level, rest.y) + oceanMoved(frame, rest, sampled, true);
    o.position = transform(frame->viewProj, float4(world, 1.0));
    o.world = world;
    o.rest = rest;
    o.currentClip = transform(frame->unjitteredViewProj, float4(world, 1.0));
    o.previousClip = transform(frame->prevViewProj, float4(before, 1.0));
    o.body = SEA;
    o.stream = 0.0;
    o.lava = 0.0;
    o.up = float3(0.0, 1.0, 0.0);
    return o;
}

// A vertex drawn nowhere: all of a quad's corners at one point outside the
// view.
WaterOut nothing()
{
    WaterOut o;
    o.position = float4(2.0, 2.0, 0.5, 1.0);
    o.world = 0.0;
    o.rest = 0.0;
    o.currentClip = o.position;
    o.previousClip = o.position;
    o.body = SEA;
    o.stream = 0.0;
    o.lava = 0.0;
    o.up = float3(0.0, 1.0, 0.0);
    return o;
}

// A corner of a quad of Pool `pool`'s grid over its footprint, at its
// height, with the sea's smallest waves on it as much as it says.
[shader("vertex")]
WaterOut poolVertex(uint vertexId : SV_VertexID, uint pool : SV_InstanceID)
{
    Frame* frame = push.frame;
    static const uint2 corners[6] = { uint2(0, 0), uint2(0, 1), uint2(1, 0), uint2(1, 0), uint2(0, 1), uint2(1, 1) };
    WaterBody body = buffers()->waterBodies[pool];
    uint quad = vertexId / 6;
    uint2 grid = uint2(quad % POOL_QUADS, quad / POOL_QUADS) + corners[vertexId % 6];
    float2 across = (float2(grid) / float(POOL_QUADS) - 0.5) * body.size.xz;
    float3 flat = transform(body.toWorld, float4(across.x, 0.0, across.y, 1.0)).xyz;
    float spacing = max(body.size.x, body.size.z) / float(POOL_QUADS);
    // Lava is too thick for waves.
    float wet = body.liquid == LIQUID_LAVA ? 0.0 : 1.0;
    float3 world = flat + oceanMoved(frame, flat.xz, spacing, false, body.waves) * wet;
    float3 before = flat + oceanMoved(frame, flat.xz, spacing, true, body.waves) * wet;
    WaterOut o;
    o.position = transform(frame->viewProj, float4(world, 1.0));
    o.world = world;
    o.rest = flat.xz;
    o.currentClip = transform(frame->unjitteredViewProj, float4(world, 1.0));
    o.previousClip = transform(frame->prevViewProj, float4(before, 1.0));
    o.body = pool;
    o.stream = 0.0;
    o.lava = 0.0;
    o.up = float3(0.0, 1.0, 0.0);
    return o;
}

// A corner of a quad of Stream push.list's strip: STREAM_ACROSS quads across
// it at each of its push.level points, as wide as each point says, flat
// across and sloping along with its bed; flowing as fast as each point says
// in the middle and a third of that at its banks.
[shader("vertex")]
WaterOut streamVertex(uint vertexId : SV_VertexID)
{
    Frame* frame = push.frame;
    static const uint2 corners[6] = { uint2(0, 0), uint2(0, 1), uint2(1, 0), uint2(1, 0), uint2(0, 1), uint2(1, 1) };
    WaterBody body = buffers()->waterBodies[push.list];
    uint quad = vertexId / 6;
    uint2 grid = uint2(quad % STREAM_ACROSS, quad / STREAM_ACROSS) + corners[vertexId % 6];
    StreamPoint p = buffers()->streamPoints[body.shape + grid.y];
    float across = float(grid.x) / float(STREAM_ACROSS) * 2.0 - 1.0;
    float2 level = normalize(p.tangent.xz + float2(0.0, 1e-5));
    float3 right = float3(level.y, 0.0, -level.x);
    float3 world = p.position + right * across * p.width * 0.5;
    float2 flow = level * p.speed * (0.3 + 0.7 * (1.0 - across * across));
    WaterOut o;
    o.position = transform(frame->viewProj, float4(world, 1.0));
    o.world = world;
    o.rest = world.xz;
    o.currentClip = transform(frame->unjitteredViewProj, float4(world, 1.0));
    o.previousClip = transform(frame->prevViewProj, float4(world, 1.0));
    o.body = push.list;
    o.stream = float4(flow, p.foam, float(min(grid.y, push.level - 1 - grid.y)));
    o.up = normalize(cross(p.tangent, right));
    o.lava = float2(p.age, across);
    return o;
}

// The waves at `rest`, with as much of the sea's long swell as `swell`
// says, added in by `weight`: their slopes, the slopes too small to see
// there as variance, how much they fold and how far they move the water.
void addWaves(Frame* frame, float2 rest, float swell, float weight, inout float2 slope, inout float variance, inout float folded, inout float3 moved)
{
    for (uint c = 0; c < OCEAN_CASCADES; c++)
    {
        float2 uv = cascadeUv(frame, c, rest);
        float4 s = textures[NonUniformResourceIndex(frame->ocean.slopes[c])].Sample(samplers[SAMPLER_LINEAR_REPEAT], uv);
        float4 d = textures[NonUniformResourceIndex(frame->ocean.displacement[c])].Sample(samplers[SAMPLER_LINEAR_REPEAT], uv);
        float share = cascadeShare(c, swell) * weight;
        slope += s.xy * share;
        // The slopes the mips averaged away, too small to see here, as roughness (Atlas).
        variance += (max(s.z - s.x * s.x, 0.0) + max(s.w - s.y * s.y, 0.0)) * share * share;
        folded += (d.w - 1.0) * share;
        moved += d.xyz * share;
    }
}

// Foam's bubbles at `rest`: small in the middle of each.
float laceAt(float2 rest, float3 moved)
{
    return waterCells(rest * 1.3 + moved.xz * 0.5) * 0.7 + waterCells(rest * 4.1) * 0.3;
}

// How far a view from `world` away from the camera (along -`v`) goes
// through Pool `pool`'s water before it leaves its box: out of its sides or
// its bottom.
float poolExit(WaterBody pool, float3 world, float3 v)
{
    float3 origin = transform(pool.toLocal, float4(world, 1.0)).xyz;
    float3 way = transform(pool.toLocal, float4(-v, 0.0)).xyz;
    float3 low = float3(-pool.size.x * 0.5, -pool.size.y, -pool.size.z * 0.5);
    float3 high = float3(pool.size.x * 0.5, 1e4, pool.size.z * 0.5);
    float3 inverse = 1.0 / (abs(way) > 1e-6 ? way : 1e-6);
    float3 far = max((low - origin) * inverse, (high - origin) * inverse);
    return max(min(far.x, min(far.y, far.z)), 0.0);
}

// Whether water drawn at `world` is kept out: inside a Dry Area; and, for
// the sea, over a Pool lower than it, which is dry to the sea.
bool keptDry(Frame* frame, float3 world, uint body)
{
    uint pools = frame->ocean.pools;
    uint dries = pools + frame->ocean.streams;
    for (uint i = dries; i < dries + frame->ocean.dries; i++)
    {
        if (dryDistance(buffers()->waterBodies[i], world) < 0.0)
        {
            return true;
        }
    }
    if (body != SEA)
    {
        return false;
    }
    for (uint i = 0; i < pools; i++)
    {
        WaterBody pool = buffers()->waterBodies[i];
        float3 local = transform(pool.toLocal, float4(world, 1.0)).xyz;
        if (pool.toWorld.c3.y < frame->ocean.level && all(abs(local.xz) <= pool.size.xz * 0.5) && local.y > -0.5)
        {
            return true;
        }
    }
    return false;
}

// The distance to the nearest of a scatter of points, a cell a unit wide:
// small in the middle of foam's bubbles.
float waterCells(float2 p)
{
    float2 n = floor(p);
    float2 f = frac(p);
    float nearest = 8.0;
    for (int j = -1; j <= 1; j++)
    {
        for (int i = -1; i <= 1; i++)
        {
            float2 g = float2(i, j);
            float2 o = frac(sin(float2(dot(n + g, float2(127.1, 311.7)), dot(n + g, float2(269.5, 183.3)))) * 43758.5453);
            float2 r = g + o - f;
            nearest = min(nearest, dot(r, r));
        }
    }
    return sqrt(nearest);
}

// Whether the camera is under the water's surface over it: the sea's, waves
// and all, or a Pool's.
bool cameraUnderwater(Frame* frame)
{
    float3 camera = frame->cameraPosition;
    float surface;
    float3 absorb;
    if (!waterSurface(frame, camera, surface, absorb))
    {
        return false;
    }
    if (frame->ocean.enabled != 0 && surface == frame->ocean.level)
    {
        surface += oceanMoved(frame, camera.xz, 0.05, false).y;
    }
    return camera.y < surface;
}

// The surface seen from under the water: inside Snell's window, the sky
// above bent through it; outside it, past the critical angle of 48.6°, the
// water below mirrored, as total internal reflection does; the under-water
// pass fogs it by how far it is.
FragmentOut fromBelow(Frame* frame, WaterOut input, float3 n, float3 v, float3 deep, float3 scatter, float3 absorb)
{
    float3 down = -n;
    float cosine = saturate(dot(down, v));
    float sine = 1.33 * sqrt(1.0 - cosine * cosine);
    bool atmosphere = frame->lighting.sky == SKY_ATMOSPHERE;
    float3 sunLight = atmosphere ? sunThroughAtmosphere(frame) : frame->sunColor;
    Surface s;
    s.normal = float3(0.0, 1.0, 0.0);
    s.view = v;
    s.roughness = 1.0;
    s.diffuse = 0.0;
    s.f0 = 0.02;
    float3 ambient = ambientLight(frame, input.world, s, sunLight, atmosphere);
    float3 water = deep * ambient + scatter * sunLight * 0.04;
    float3 color = water;
    if (sine < 1.0)
    {
        float3 bent = refract(-v, down, 1.33);
        float3 sky = surroundingsReflection(frame, input.world, n, bent, 0.05, ambient, atmosphere);
        // The sun's disc through the window, where the bent view meets it.
        sky += sunLight * pow(saturate(dot(bent, -frame->sunDirection)), 800.0) * 40.0;
        // Reflecting more towards the window's edge, wholly past it.
        float edge = pow(saturate(sine), 6.0);
        float fresnel = lerp(0.02 + 0.98 * pow(1.0 - cosine, 5.0), 1.0, edge);
        color = lerp(sky, water, fresnel);
    }
    FragmentOut o;
    o.color = float4(color, 1.0);
    o.velocity = motion(input.currentClip, input.previousClip);
    return o;
}

// The sea's surface: what lies under it, seen through it, bent by its
// waves and lost a colour at a time with depth (push.texture the picture
// behind it with mips, push.textures.y the depth behind it); the water's
// own colour, deep blue lit by the sky, and teal where light comes through
// the thin top of a wave (Sea of Thieves' blend, with Atlas's scatter); the
// sky and the sun reflected, sharp as water's 2%, the sun's glints as rough
// as the waves too small to see make it; and foam where the crests fold.
[shader("fragment")]
FragmentOut waterFragment(WaterOut input)
{
    Frame* frame = push.frame;
    if (keptDry(frame, input.world, input.body))
    {
        discard;
    }
    bool sea = input.body == SEA;
    WaterBody pool = buffers()->waterBodies[sea ? 0 : input.body];
    bool stream = !sea && pool.kind == WATER_STREAM;
    if (!sea && pool.liquid == LIQUID_LAVA)
    {
        return lavaFragment(frame, input, pool, stream);
    }
    // A Pool has the sea's waves as much as it says, and its own colours.
    float waves = sea ? 1.0 : pool.waves;
    float3 deep = sea ? frame->ocean.deep : pool.deep;
    float3 scatter = sea ? frame->ocean.scatter : pool.scatter;
    float3 absorb = sea ? frame->ocean.absorb : pool.absorb;
    float2 rest = input.rest;
    float2 slope = 0.0;
    float variance = 0.0;
    float folded = 1.0;
    float3 moved = 0.0;
    // A Stream's ripples drift down it (two-phase flow, after Vlachos): two sets, each carried along for a period
    // and then started again, half a period apart, each faded out as it starts again.
    float2 flow = input.stream.xy;
    float phase = frac(frame->ocean.time / FLOW_PERIOD);
    float2 restA = rest - flow * phase * FLOW_PERIOD;
    float2 restB = rest - flow * frac(phase + 0.5) * FLOW_PERIOD;
    float blend = abs(1.0 - 2.0 * phase);
    if (stream)
    {
        addWaves(frame, restA, 0.0, 1.0 - blend, slope, variance, folded, moved);
        addWaves(frame, restB, 0.0, blend, slope, variance, folded, moved);
        // Faster water is rougher.
        slope *= 0.6 + 0.25 * length(flow);
    }
    else
    {
        addWaves(frame, rest, waves, 1.0, slope, variance, folded, moved);
    }
    float3 n = normalize(float3(-slope.x, 0.0, -slope.y) + input.up);
    float3 v = normalize(frame->cameraPosition - input.world);
    // From under the water the surface shows its underside; from the air, a wave above the camera its front.
    if (cameraUnderwater(frame))
    {
        return fromBelow(frame, input, n, v, deep, scatter, absorb);
    }
    float3 l = -frame->sunDirection;
    bool atmosphere = frame->lighting.sky == SKY_ATMOSPHERE;
    float3 sunLight = atmosphere ? sunThroughAtmosphere(frame) : frame->sunColor;
    float shadow = sunShadow(frame, input.world, float3(0.0, 1.0, 0.0));
    float alpha = sqrt(0.02 * 0.02 + 2.0 * variance);

    Surface s;
    s.normal = n;
    s.view = v;
    s.roughness = clamp(sqrt(alpha), 0.03, 1.0);
    s.diffuse = 0.0;
    s.f0 = 0.02;
    float nv = saturate(dot(n, v));
    float fresnel = 0.02 + 0.98 * pow(1.0 - nv, 5.0);
    float3 ambient = ambientLight(frame, input.world, s, sunLight, atmosphere);
    float3 reflected = surroundingsReflection(frame, input.world, n, reflect(-v, n), s.roughness, ambient, atmosphere);
    // The sun, a little rougher than the water so its disc shows as a streak of glints rather than a point.
    Surface sunSurface = s;
    sunSurface.roughness = max(s.roughness, 0.08);
    float3 glints = shade(sunSurface, l, sunLight) * shadow;

    // The water's own colour (Sea of Thieves, Atlas): deep blue lit by the sky, and the scatter colour where the
    // sun shines through the thin top of a wave towards the eye.
    float height = max(frame->ocean.waveHeight * max(waves, 0.2), 0.05);
    // A Stream's ripples have no crests for light to come through.
    float crest = stream ? 0.0 : saturate(moved.y / (height * 0.5)) + saturate(length(moved.xz) / height) * 0.5;
    float towards = pow(saturate(dot(l, -v)), 4.0) * pow(saturate(0.5 - 0.5 * dot(l, n)), 3.0);
    // Sea of Thieves' crests glow through: strongly towards the sun, and a little from every side on a wave's top.
    float3 throughWave = (3.0 * crest * towards + 0.25 * crest * pow(saturate(dot(v, n)), 2.0) + 0.04 * saturate(dot(l, n))) * scatter * sunLight * shadow;
    float3 body = deep * ambient + throughWave;

    // What lies under it: how far through the water, bent by the waves, and dimmed a colour at a time.
    float2 pixel = input.position.xy;
    float behindDepth = textures[NonUniformResourceIndex(push.textures.y)].Load(int3(int2(pixel), 0)).r;
    float surfaceDepth = frame->near / max(input.position.z, 1e-7);
    float sceneDepth = behindDepth > 0.0 ? frame->near / behindDepth : 1e4;
    float along = max(dot(-v, frame->cameraForward), 0.05);
    float thickness = max(sceneDepth - surfaceDepth, 0.0) / along;
    // A Pool's water ends at its sides and bottom, whatever lies past them; a Stream's at its bed.
    thickness = sea || stream ? thickness : min(thickness, poolExit(pool, input.world, v));
    float3 through = exp(-absorb * thickness);
    float2 bend = n.xz * 0.04 * saturate(thickness);
    // As sharp as clear water shows what is under it, blurring only where it is deep or rough.
    float blur = sqrt(s.roughness) * 1.2 + log2(1.0 + thickness * 0.12);
    float3 behind = thickness < 1e3 ? glassBehind(push.texture, pixel / float2(frame->viewport) - bend, blur) : 0.0;
    float3 under = behind * through + body * (1.0 - through);
    float3 color = (1.0 - fresnel) * under + fresnel * reflected + glints;

    // Foam where a crest folds over, and where the water thins at a shore, in bubbles drifting with the waves.
    // Only where the water folds hard does a crest break: a calm sea's gentle folding stays clear.
    float threshold = 0.4 + 0.45 * frame->ocean.foam;
    float breaking = frame->ocean.foam > 0.0 ? saturate((threshold - folded) / 0.3) : 0.0;
    float shore = saturate(1.0 - thickness / 0.6);
    float lace = laceAt(rest, moved);
    if (stream)
    {
        // A Stream's foam where it runs steep, and in lines drawn out along it the faster it runs, carried down it
        // as its ripples are.
        float speed = length(flow);
        float2 along = flow / max(speed, 1e-4);
        float2 across = float2(-along.y, along.x);
        float2 a = float2(dot(restA, along) * 0.25, dot(restA, across));
        float2 b = float2(dot(restB, along) * 0.25, dot(restB, across));
        breaking = max(input.stream.z, 0.3 * saturate(speed / 4.0));
        lace = lerp(laceAt(a, 0.0), laceAt(b, 0.0), blend);
    }
    // What breaking crests left, carried with the water, streaked thin as it spreads (the sea's only).
    float trail = sea ? keptFoam(frame, rest) : 0.0;
    float foam = saturate((max(max(breaking, shore), trail * 1.2) * 1.4 - lace) * 6.0);
    float3 foamLit = 0.85 * (ambient + sunLight * shadow * saturate(dot(n, l)) / PI);
    color = lerp(color, foamLit, foam);
    // A Stream fades in over its first two points and out over its last two.
    if (stream)
    {
        color = lerp(glassBehind(push.texture, pixel / float2(frame->viewport), 0.0), color, saturate(input.stream.w / 2.0));
    }

    if (atmosphere)
    {
        float4 haze = aerialPerspectiveAt(frame, pixel / float2(frame->viewport), length(input.world - frame->cameraPosition));
        color = color * haze.a + haze.rgb * skyLook(frame);
    }
    FragmentOut o;
    o.color = float4(color, 1.0);
    o.velocity = motion(input.currentClip, input.previousClip);
    return o;
}

// ---------------------------------------------------------------- Lava

// The light of something as hot as `kelvin`, by Planck's law at red's,
// green's and blue's wavelengths: 1 in red at 1400 K, where it is orange;
// yellow and far brighter hotter; dull red, and dimming fast, as it cools.
float3 blackbody(float kelvin)
{
    static const float3 wavelength = float3(610e-9, 550e-9, 465e-9);
    float3 shorter = pow(wavelength / 610e-9, 5.0);
    float3 planck = 1.0 / (shorter * (exp(1.4388e-2 / (wavelength * max(kelvin, 300.0))) - 1.0));
    return planck * (exp(1.4388e-2 / (610e-9 * 1400.0)) - 1.0);
}

float2 cellPoint(float2 cell)
{
    return frac(sin(float2(dot(cell, float2(127.1, 311.7)), dot(cell, float2(269.5, 183.3)))) * 43758.5453);
}

// How far `p` is from the nearest edge between a scatter of cells a unit
// wide (x), and which cell it is in, from 0 to 1 (y).
float2 cellEdge(float2 p)
{
    float2 n = floor(p);
    float2 f = frac(p);
    float2 nearest = 0.0;
    float2 nearestCell = 0.0;
    float best = 8.0;
    for (int j = -1; j <= 1; j++)
    {
        for (int i = -1; i <= 1; i++)
        {
            float2 g = float2(i, j);
            float2 r = g + cellPoint(n + g) - f;
            float d = dot(r, r);
            if (d < best)
            {
                best = d;
                nearest = r;
                nearestCell = n + g;
            }
        }
    }
    // Half way to each neighbour, across the line between them.
    float edge = 8.0;
    for (int j = -2; j <= 2; j++)
    {
        for (int i = -2; i <= 2; i++)
        {
            float2 g = nearestCell - n + float2(i, j);
            float2 r = g + cellPoint(n + g) - f;
            float2 between = r - nearest;
            if (dot(between, between) > 1e-5)
            {
                edge = min(edge, dot(0.5 * (nearest + r), normalize(between)));
            }
        }
    }
    return float2(edge, cellPoint(nearestCell).x);
}

// Smooth noise from 0 to 1, a unit across.
float lavaNoise(float2 p)
{
    float2 n = floor(p);
    float2 f = frac(p);
    f = f * f * (3.0 - 2.0 * f);
    float a = cellPoint(n).x;
    float b = cellPoint(n + float2(1, 0)).x;
    float c = cellPoint(n + float2(0, 1)).x;
    float d = cellPoint(n + float2(1, 1)).x;
    return lerp(lerp(a, b, f.x), lerp(c, d, f.x), f.y);
}

// `up` tilted by how `height` changes across the pixel at `world` (a
// surface gradient, after Mikkelsen), for bumps no mesh has.
float3 bumped(float3 world, float3 up, float height)
{
    float3 dx = ddx(world);
    float3 dy = ddy(world);
    float3 r1 = cross(dy, up);
    float3 r2 = cross(up, dx);
    float det = dot(dx, r1);
    float3 gradient = (det < 0.0 ? -1.0 : 1.0) * (ddx(height) * r1 + ddy(height) * r2);
    return normalize(max(abs(det), 1e-10) * up - gradient);
}

// Lava's surface (`body` a Pool or, with `stream`, a Stream of it): molten
// where it is fresh, glowing by its temperature; crusting over as it
// cools, in plates carried along with it, cracked open where the molten
// lava under them shows through, the cracks narrowing as it cools; along a
// Stream, its skin pushed into ropes across it as it slows, bowed
// downstream as the middle runs faster than the banks (pahoehoe). A Pool is
// a lake of it, its plates drifting as it turns over. Under the sea it
// crusts over at once. Opaque, lit as dark glassy rock where it has
// crusted.
FragmentOut lavaFragment(Frame* frame, WaterOut input, WaterBody body, bool stream)
{
    float cooling = body.lava.x;
    float glow = body.lava.y;
    float t = frame->time;
    float2 p = input.rest;
    float age;
    float2 plate;
    float plateSize;
    // How far behind the middle the ropes are at the banks, in seconds.
    float bow = 0.0;
    if (stream)
    {
        float across = input.lava.y;
        // Its banks crust over first, as they are thinner and slower, into levees either side of a molten middle.
        age = input.lava.x / (0.5 + 0.5 * (1.0 - across * across));
        float2 along = input.stream.xy / max(length(input.stream.xy), 1e-4);
        // Its crust rides along as one plug with its middle, so its plates are where they are by when the lava
        // there set out, in seconds: as many a metre as it is slow, pushed together where it slows.
        plate = float2(t - input.lava.x, dot(p, float2(-along.y, along.x)));
        plateSize = 1.2;
        bow = 1.2 * across * across;
    }
    else
    {
        // A lake: its plates drifting, its surface turning over in places every quarter of a minute or so, so a
        // shorter Cooling crusts more of it over.
        plate = p + float2(sin(t * 0.05), cos(t * 0.043)) * 3.0 + float2(t * 0.02, 0.0);
        age = LAKE_TURNOVER * (0.5 + 1.2 * lavaNoise(p * 0.08 + t * 0.01));
        plateSize = 3.0;
    }
    if (frame->ocean.enabled != 0 && input.world.y < frame->ocean.level)
    {
        age = cooling * 4.0;
    }
    float cooled = age / cooling;
    float crust = smoothstep(0.15, 0.7, cooled);
    // Plates as irregular as torn crust is: their shapes bent by noise, their cracks wider in places.
    float2 scaled = plate / plateSize;
    scaled += (float2(lavaNoise(scaled * 0.7), lavaNoise(scaled * 0.7 + 17.0)) - 0.5) * 0.8;
    float2 cell = cellEdge(scaled);
    float crackWidth = (0.12 * (1.0 - saturate(cooled / 3.0)) + 0.015) * (0.4 + 1.2 * lavaNoise(scaled * 1.3 + 5.0));
    float crack = 1.0 - smoothstep(crackWidth * 0.5, crackWidth, cell.x);
    float skin = crust * (1.0 - crack);
    // Pahoehoe's ropes: the skin folded while it is still soft, before it sets, hotter in their troughs.
    float soft = smoothstep(0.1, 0.4, cooled) * (1.0 - smoothstep(1.5, 3.0, cooled));
    float ropes = stream ? sin(6.2832 * (plate.x + bow) / 0.7 + 3.0 * lavaNoise(p * 0.4)) * soft : 0.0;

    // Hottest inside, and in the middle of a crack; its skin cooling fast once it forms, the tops of its ropes
    // first, in patches.
    float molten = 1440.0 - 70.0 * saturate(cooled * 0.5) - 60.0 * lavaNoise(plate * 1.7) - 180.0 * saturate(cell.x / max(crackWidth, 1e-3)) * crust;
    float patches = lavaNoise(p * 2.3) * 120.0;
    float skinKelvin = lerp(1230.0, 420.0, saturate((cooled - 0.15) * 1.2)) - 90.0 * max(ropes, 0.0) + 160.0 * max(-ropes, 0.0) - patches;
    float3 emitted = blackbody(lerp(molten, skinKelvin, skin)) * glow * 4.0;

    float rough = lavaNoise(p * 7.0) * 0.5 + lavaNoise(p * 19.0) * 0.25;
    float height = ropes * 0.06 * skin + (cell.x * 0.12 + rough * 0.03) * crust;
    float3 n = bumped(input.world, input.up, height);
    float3 v = normalize(frame->cameraPosition - input.world);
    float3 l = -frame->sunDirection;
    bool atmosphere = frame->lighting.sky == SKY_ATMOSPHERE;
    float3 sunLight = atmosphere ? sunThroughAtmosphere(frame) : frame->sunColor;
    float shadow = sunShadow(frame, input.world, n);
    Surface s;
    s.normal = n;
    s.view = v;
    // Basalt nearly black, its fresh skin glassy.
    s.diffuse = lerp(0.03, 0.06, cell.y) * skin;
    s.f0 = 0.04;
    s.roughness = lerp(0.7, 0.3, skin * (1.0 - saturate(cooled - 1.0)));
    float3 ambient = ambientLight(frame, input.world, s, sunLight, atmosphere);
    float fresnel = 0.04 + 0.96 * pow(1.0 - saturate(dot(n, v)), 5.0);
    float3 reflected = surroundingsReflection(frame, input.world, n, reflect(-v, n), s.roughness, ambient, atmosphere);
    float3 color = s.diffuse * ambient + shade(s, l, sunLight) * shadow + fresnel * reflected * skin + emitted;

    float2 pixel = input.position.xy;
    if (stream)
    {
        color = lerp(glassBehind(push.texture, pixel / float2(frame->viewport), 0.0), color, saturate(input.stream.w / 2.0));
    }
    if (atmosphere)
    {
        float4 haze = aerialPerspectiveAt(frame, pixel / float2(frame->viewport), length(input.world - frame->cameraPosition));
        color = color * haze.a + haze.rgb * skyLook(frame);
    }
    FragmentOut o;
    o.color = float4(color, 1.0);
    o.velocity = motion(input.currentClip, input.previousClip);
    return o;
}

// ---------------------------------------------------------------- Walls of water

struct WallOut
{
    float4 position : SV_Position;
    float3 world : POSITION;
    // The face's normal, out of the body's box.
    float3 normal : NORMAL;
    float4 currentClip : CURRENT_CLIP;
    float4 previousClip : PREVIOUS_CLIP;
    nointerpolation uint body : BODY;
};

WallOut nothingWall()
{
    WallOut o;
    o.position = float4(2.0, 2.0, 0.5, 1.0);
    o.world = 0.0;
    o.normal = float3(0.0, 1.0, 0.0);
    o.currentClip = o.position;
    o.previousClip = o.position;
    o.body = 0;
    return o;
}

// A corner of a face of body `body`'s box: a Dry Area's box, whose inside
// faces show the water round it, as a parted sea's walls; or a Pool's,
// from just over its surface down as deep as it is, whose outside faces
// and bottom show the water in it from the side and from below.
[shader("vertex")]
WallOut wallVertex(uint vertexId : SV_VertexID, uint instance : SV_InstanceID)
{
    Frame* frame = push.frame;
    // The bodies drawn start at push.list: the Dry Areas with the sea, the Pools after it.
    uint body = push.list + instance;
    // Each face's normal and two edges, then its two triangles' corners.
    static const float3 normals[6] = { float3(1, 0, 0), float3(-1, 0, 0), float3(0, 1, 0), float3(0, -1, 0), float3(0, 0, 1), float3(0, 0, -1) };
    static const float3 us[6] = { float3(0, 0, 1), float3(0, 0, -1), float3(1, 0, 0), float3(1, 0, 0), float3(-1, 0, 0), float3(1, 0, 0) };
    static const float2 corners[6] = { float2(-1, -1), float2(1, -1), float2(-1, 1), float2(-1, 1), float2(1, -1), float2(1, 1) };
    WaterBody water = buffers()->waterBodies[body];
    uint face = vertexId / 6;
    float3 n = normals[face];
    float3 u = us[face];
    float3 v = cross(n, u);
    float2 c = corners[vertexId % 6];
    float3 local = n + u * c.x + v * c.y;
    float3 half;
    float3 centre;
    if (water.kind == WATER_POOL)
    {
        // From a little over its surface, past its highest ripples, down as deep as it is; no lid.
        if (face == 2)
        {
            return nothingWall();
        }
        float depth = water.size.y;
        half = float3(water.size.x * 0.5, (depth + 0.5) * 0.5, water.size.z * 0.5);
        centre = float3(0.0, (0.5 - depth) * 0.5, 0.0);
    }
    else
    {
        half = water.size * 0.5;
        centre = 0.0;
    }
    float3 world = transform(water.toWorld, float4(centre + local * half, 1.0)).xyz;
    WallOut o;
    o.position = transform(frame->viewProj, float4(world, 1.0));
    o.world = world;
    o.normal = normalize(transform(water.toWorld, float4(n, 0.0)).xyz);
    o.currentClip = transform(frame->unjitteredViewProj, float4(world, 1.0));
    o.previousClip = transform(frame->prevViewProj, float4(world, 1.0));
    o.body = body;
    return o;
}

// How high the water a wall holds stands above `world`: a Pool's surface,
// with its share of the sea's small waves, or the sea's round a Dry Area.
float wallSurface(Frame* frame, WaterBody water, float3 world)
{
    if (water.kind == WATER_POOL)
    {
        float wet = water.liquid == LIQUID_LAVA ? 0.0 : 1.0;
        return water.toWorld.c3.y + oceanMoved(frame, world.xz, 0.25, false, water.waves).y * wet;
    }
    return frame->ocean.level + oceanMoved(frame, world.xz, 0.25, false).y;
}

// A lava Pool's side: dark rock in plates, cracked, the cracks glowing the
// hotter the deeper they are, as its outside has cooled and its inside has not.
FragmentOut lavaWall(Frame* frame, WallOut input, WaterBody lava, float below)
{
    float2 across = float2(dot(input.world.xz, float2(input.normal.z, -input.normal.x)), input.world.y);
    float2 cell = cellEdge(across / 0.9);
    float crack = 1.0 - smoothstep(0.02, 0.05, cell.x);
    float kelvin = lerp(600.0, 1350.0, saturate(below / max(lava.size.y, 0.1) + 0.3));
    float3 emitted = blackbody(kelvin) * crack * lava.lava.y * 4.0;
    float3 v = normalize(frame->cameraPosition - input.world);
    bool atmosphere = frame->lighting.sky == SKY_ATMOSPHERE;
    float3 sunLight = atmosphere ? sunThroughAtmosphere(frame) : frame->sunColor;
    Surface s;
    s.normal = bumped(input.world, input.normal, cell.x * 0.15);
    s.view = v;
    s.diffuse = lerp(0.03, 0.06, cell.y);
    s.f0 = 0.04;
    s.roughness = 0.7;
    float3 ambient = ambientLight(frame, input.world, s, sunLight, atmosphere);
    float3 color = s.diffuse * ambient + shade(s, -frame->sunDirection, sunLight) * sunShadow(frame, input.world, s.normal) + emitted;
    FragmentOut o;
    o.color = float4(color, 1.0);
    o.velocity = motion(input.currentClip, input.previousClip);
    return o;
}

// The water a wall holds, seen from the side: what lies in it and behind
// it, through as much water as lies between, lost a colour at a time, and
// the water's own colour, lit by the sky and darker the deeper it is; a
// bright line where it meets its surface.
[shader("fragment")]
FragmentOut wallFragment(WallOut input)
{
    Frame* frame = push.frame;
    WaterBody water = buffers()->waterBodies[input.body];
    float3 v = normalize(frame->cameraPosition - input.world);
    bool facing = dot(input.normal, v) > 0.0;
    // A Dry Area shows its far walls from inside it; a Pool its near walls from outside it.
    bool pool = water.kind == WATER_POOL;
    if (pool != facing)
    {
        discard;
    }
    float surface = wallSurface(frame, water, input.world);
    if (input.world.y > surface || (!pool && frame->ocean.enabled == 0))
    {
        discard;
    }
    if (pool && water.liquid == LIQUID_LAVA)
    {
        return lavaWall(frame, input, water, surface - input.world.y);
    }
    float3 deep = pool ? water.deep : frame->ocean.deep;
    float3 scatter = pool ? water.scatter : frame->ocean.scatter;
    float3 absorb = pool ? water.absorb : frame->ocean.absorb;

    float2 pixel = input.position.xy;
    float behindDepth = textures[NonUniformResourceIndex(push.textures.y)].Load(int3(int2(pixel), 0)).r;
    float wallDepth = frame->near / max(input.position.z, 1e-7);
    float sceneDepth = behindDepth > 0.0 ? frame->near / behindDepth : 1e4;
    float along = max(dot(-v, frame->cameraForward), 0.05);
    float thickness = max(sceneDepth - wallDepth, 0.0) / along;
    thickness = pool ? min(thickness, poolExit(water, input.world, v)) : thickness;
    float3 through = exp(-absorb * thickness);
    float3 behind = thickness < 1e3 ? glassBehind(push.texture, pixel / float2(frame->viewport), log2(1.0 + thickness * 0.3)) : 0.0;

    bool atmosphere = frame->lighting.sky == SKY_ATMOSPHERE;
    float3 sunLight = atmosphere ? sunThroughAtmosphere(frame) : frame->sunColor;
    Surface s;
    s.normal = float3(0.0, 1.0, 0.0);
    s.view = v;
    s.roughness = 1.0;
    s.diffuse = 0.0;
    s.f0 = 0.02;
    float3 ambient = ambientLight(frame, input.world, s, sunLight, atmosphere);
    // Light from above, dimmed by the water it came down through.
    float below = max(surface - input.world.y, 0.0);
    float3 reaching = exp(-absorb * below);
    float3 body = (deep * ambient + scatter * sunLight * 0.05) * (0.35 + 0.65 * reaching);
    float3 color = behind * through + body * (1.0 - through);
    // The water's edge against its surface, bright and frothy.
    float rim = saturate(1.0 - below / 0.06);
    color = lerp(color, ambient * 0.9 + sunLight * 0.1, rim * 0.7);
    if (atmosphere)
    {
        float4 haze = aerialPerspectiveAt(frame, pixel / float2(frame->viewport), length(input.world - frame->cameraPosition));
        color = color * haze.a + haze.rgb * skyLook(frame);
    }
    FragmentOut o;
    o.color = float4(color, 1.0);
    o.velocity = motion(input.currentClip, input.previousClip);
    return o;
}

// ---------------------------------------------------------------- Under water

struct UnderOut
{
    float4 position : SV_Position;
};

// A triangle over the whole view.
[shader("vertex")]
UnderOut underwaterVertex(uint id : SV_VertexID)
{
    float2 corner = float2((id << 1) & 2, id & 2);
    UnderOut o;
    o.position = float4(corner * 2.0 - 1.0, 0.0, 1.0);
    return o;
}

// The water a pixel's view starts in: whether the camera's near plane there
// is under it, as the camera half in the sea is under it below the
// waterline; how much of what the pixel shows comes through the water as
// far as it, and the water's own light over that far; and how near the
// near plane is to the surface, for the waterline's dark meniscus.
struct Under
{
    bool under;
    float3 through;
    float3 glow;
    float meniscus;
};

Under underwaterAt(Frame* frame, float2 pixel)
{
    Under u;
    u.under = false;
    u.through = 1.0;
    u.glow = 0.0;
    u.meniscus = 0.0;
    float2 ndc = pixel / float2(frame->viewport) * 2.0 - 1.0;
    float4 nearH = transform(frame->invViewProj, float4(ndc, 1.0, 1.0));
    float3 near = nearH.xyz / nearH.w;
    float surface;
    float3 absorb;
    if (!waterSurface(frame, near, surface, absorb))
    {
        return u;
    }
    // Its waves too, for the sea, so the waterline rises and falls with them.
    if (frame->ocean.enabled != 0 && surface == frame->ocean.level)
    {
        surface += oceanMoved(frame, near.xz, 0.05, false).y;
    }
    u.meniscus = saturate(1.0 - abs(near.y - surface) / 0.015);
    if (near.y >= surface)
    {
        return u;
    }
    u.under = true;
    float depth = textures[NonUniformResourceIndex(push.textures.x)].Load(int3(int2(pixel), 0)).r;
    float distance = 1e4;
    if (depth > 0.0)
    {
        float4 farH = transform(frame->invViewProj, float4(ndc, depth, 1.0));
        distance = length(farH.xyz / farH.w - frame->cameraPosition);
    }
    u.through = exp(-absorb * distance);
    // The water's own colours: a Pool's, or the sea's.
    float3 deep = frame->ocean.deep;
    float3 scatter = frame->ocean.scatter;
    for (uint i = 0; i < frame->ocean.pools; i++)
    {
        WaterBody pool = buffers()->waterBodies[i];
        float3 local = transform(pool.toLocal, float4(near, 1.0)).xyz;
        if (pool.liquid == LIQUID_WATER && all(abs(local.xz) <= pool.size.xz * 0.5) && local.y < 0.0 && local.y > -pool.size.y)
        {
            deep = pool.deep;
            scatter = pool.scatter;
        }
    }
    bool atmosphere = frame->lighting.sky == SKY_ATMOSPHERE;
    float3 sunLight = atmosphere ? sunThroughAtmosphere(frame) : frame->sunColor;
    Surface s;
    s.normal = float3(0.0, 1.0, 0.0);
    s.view = float3(0.0, 1.0, 0.0);
    s.roughness = 1.0;
    s.diffuse = 0.0;
    s.f0 = 0.02;
    float3 ambient = ambientLight(frame, near, s, sunLight, atmosphere);
    // Darker the deeper the camera; brighter looking towards the sun, as light scatters forwards.
    float3 dim = exp(-absorb * max(surface - near.y, 0.0) * 0.5);
    float3 view = normalize(near - frame->cameraPosition);
    float forward = pow(saturate(dot(view, -frame->sunDirection)), 4.0);
    float3 own = deep * ambient + scatter * sunLight * (0.03 + 0.12 * forward);
    u.glow = own * dim * (1.0 - u.through);
    return u;
}

// Dims what the view shows under water by the water it comes through,
// colour by colour (blended as a filter), and darkens the waterline.
[shader("fragment")]
float4 underwaterDimFragment(UnderOut input) : SV_Target0
{
    Under u = underwaterAt(push.frame, input.position.xy);
    if (!u.under && u.meniscus <= 0.0)
    {
        discard;
    }
    return float4(u.through * (1.0 - u.meniscus * 0.75), 1.0);
}

// Adds the water's own light over what the view shows under water.
[shader("fragment")]
float4 underwaterGlowFragment(UnderOut input) : SV_Target0
{
    Under u = underwaterAt(push.frame, input.position.xy);
    if (!u.under)
    {
        discard;
    }
    return float4(u.glow, 1.0);
}
