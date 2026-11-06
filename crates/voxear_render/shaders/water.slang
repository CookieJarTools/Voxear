// The sea (src/ocean.rs, src/water.rs). Its waves are an FFT of an ocean
// spectrum: six cascades, each moved on in time and summed by an inverse
// FFT along its rows then its columns (oceanRowsMain, oceanColumnsMain),
// into how far the water at each point is moved, how much it folds at a
// crest, and its slopes, with mips (oceanMipMain). The surface is a grid of
// rings round the camera, each twice as coarse as the one inside it
// (waterVertex), lit in waterFragment.

#include "glass.slang"

// The cascades' resolution and how many there are (OCEAN_CASCADES in src/ocean.rs).
static const uint OCEAN_N = 256;
static const uint OCEAN_CASCADES = 6;

// The costlier parts of the sea's look, as bits (src/ocean.rs's features).
static const uint OCEAN_SHORES = 1;
static const uint OCEAN_FOAM_TRAILS = 2;
static const uint OCEAN_FOAM_3D = 4;
static const uint OCEAN_LIGHT_BENDING = 8;
static const uint OCEAN_LIGHT_SHAFTS = 16;
static const uint OCEAN_WET_LENS = 32;
static const uint OCEAN_FINE_RIPPLES = 64;

// Whether the sea has `feature` on.
bool oceanHas(Frame* frame, uint feature)
{
    return (frame->ocean.features & feature) != 0;
}

// How many of the cascades are worked out: all but the finest two without Fine Ripples.
uint oceanCascades(Frame* frame)
{
    return oceanHas(frame, OCEAN_FINE_RIPPLES) ? OCEAN_CASCADES : OCEAN_CASCADES - 2;
}
// The cascades whose folding breaks into foam: the ripples' never does.
static const uint OCEAN_BREAKING_CASCADES = 4;
// The sea floor round the camera, seen from straight above (src/water.rs): this many slices, each this many texels a
// side, two a side in one depth image, its heights kept within this many metres of the sea's level as its depth.
static const uint FLOOR_SLICES = 4;
static const uint FLOOR_TEXELS = 256;
static const float FLOOR_REACH = 256.0;
// Water deeper than this, in metres, is deep to every wave (ocean::SHALLOWS_DEPTH).
static const float SHALLOWS_DEPTH = 40.0;
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
// How much of every colour a metre of water scatters away under it, beside
// what it absorbs: its specks of sediment and plankton, which no water is
// without.
static const float UNDERWATER_SCATTERING = 0.05;

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

// A wave number's frequency in deep water, by gravity and, for ripples,
// surface tension, rounded to a whole number of turns in OCEAN_REPEAT, as
// the CPU's (ocean::frequency).
float waveFrequency(float k)
{
    float step = 6.2831853 / OCEAN_REPEAT;
    return floor(sqrt(GRAVITY * k + 7.4e-5 * k * k * k) / step) * step;
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
    // The spectrum is the waves at the height their winds raise them; how much higher they are is applied here.
    float scale = frame->ocean.waveScale;
    for (uint e = 0; e < 2; e++)
    {
        uint z = local.x + e * 128;
        float2 a = oceanRow[0][z] * scale;
        float2 b = oceanRow[1][z] * scale;
        float2 c = oceanRow[2][z] * scale;
        float2 d = oceanRow[3][z] * scale;
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

// Four mips at once, each texel the average of the four under it: from mip
// push.level of push.texture into the storage images push.output (the
// first, push.size across) and push.textures.xyz (the next three), each
// group of 8×8 threads reading 16×16 texels and handing its sums on to the
// next mip through shared memory.
groupshared float4 mipTile[8][8];

[shader("compute")]
[numthreads(8, 8, 1)]
void oceanMipsMain(uint3 id : SV_DispatchThreadID, uint3 group : SV_GroupID, uint3 local : SV_GroupThreadID)
{
    Texture2D source = textures[NonUniformResourceIndex(push.texture)];
    int2 at = int2(id.xy) * 2;
    float4 sum = (source.Load(int3(at, push.level)) + source.Load(int3(at + int2(1, 0), push.level))
        + source.Load(int3(at + int2(0, 1), push.level)) + source.Load(int3(at + int2(1, 1), push.level))) * 0.25;
    if (all(id.xy < push.size))
    {
        storageImagesRgba16f[push.output][id.xy] = sum;
    }
    mipTile[local.y][local.x] = sum;
    uint outputs[3] = { push.textures.x, push.textures.y, push.textures.z };
    uint wide = 4;
    for (uint step = 0; step < 3; step++)
    {
        GroupMemoryBarrierWithGroupSync();
        float4 next = 0.0;
        if (all(local.xy < wide))
        {
            uint2 from = local.xy * 2;
            next = (mipTile[from.y][from.x] + mipTile[from.y][from.x + 1] + mipTile[from.y + 1][from.x] + mipTile[from.y + 1][from.x + 1]) * 0.25;
        }
        GroupMemoryBarrierWithGroupSync();
        if (all(local.xy < wide))
        {
            mipTile[local.y][local.x] = next;
            uint2 texel = group.xy * wide + local.xy;
            if (all(texel < max(push.size >> (step + 1), 1)))
            {
                storageImagesRgba16f[outputs[step]][texel] = next;
            }
        }
        wide /= 2;
    }
}

// The foam round the camera: a square of FOAM_TEXELS a side, FOAM_TEXEL
// metres each, kept from frame to frame.
static const uint FOAM_TEXELS = 512;
static const float FOAM_TEXEL = 0.5;

// ---------------------------------------------------------------- The sea floor

// How deep in the sea floor's depth image ground `y` high is drawn: the
// higher, the deeper, so the highest ground over a place stays.
float floorDepth(Frame* frame, float y)
{
    return saturate((y - frame->ocean.level + FLOOR_REACH) / (2.0 * FLOOR_REACH));
}

// A vertex of instance push.source seen from straight above, across slice
// push.level of the sea floor.
[shader("vertex")]
float4 floorVertex(uint vertexId : SV_VertexID) : SV_Position
{
    Frame* frame = push.frame;
    Instance instance = buffers()->instances[push.source];
    Mesh mesh = buffers()->meshes[instance.mesh];
    Vertex v = buffers()->vertices[mesh.vertexOffset + vertexId];
    float3 world = transform(instance.model, float4(v.position, 1.0)).xyz;
    float4 slice = frame->ocean.floor[push.level];
    return float4((world.xz - slice.xy) / slice.z * 2.0 - 1.0, floorDepth(frame, world.y), 1.0);
}

// How high the ground is at `uv` across slice `slice` of the sea floor,
// read smoothly with `smooth`, so its outline does not step from texel to
// texel where foam follows it; as deep as the floor reaches where nothing
// stands. With `under`, how high its underside is, as high as the floor
// reaches where nothing stands.
float floorSliceHeight(Frame* frame, uint slice, float2 uv, bool smooth, bool under = false)
{
    float2 tile = float2(float(slice % 2 + (under ? 2 : 0)), float(slice / 2));
    Texture2D floor = textures[NonUniformResourceIndex(frame->ocean.floorTexture)];
    float2 at = (tile + uv) * 0.25;
    float d = smooth ? smoothSample(floor, at, float(FLOOR_TEXELS * 4)).r : floor.SampleLevel(samplers[SAMPLER_LINEAR_CLAMP], at, 0.0).r;
    return frame->ocean.level - FLOOR_REACH + d * 2.0 * FLOOR_REACH;
}

// The ground under the water at `xz`, `y` high: how high its top is, where
// it reaches down into the water there; very far down where it does not, as
// under an overhang, where the water carries on beneath it.
float groundUnder(Frame* frame, float2 xz, float y, bool smooth = true)
{
    float top = floorHeight(frame, xz, smooth);
    return top > y && floorHeight(frame, xz, false, true) > y ? -1e9 : top;
}

// How high the ground is under `xz`, from the finest slice of the sea floor
// that reaches it, fading into the next one out towards its edge so no
// seam shows where they meet; very far down beyond them all. Read smoothly
// unless told not to, where nothing shows its outline. With `under`, how
// high its underside is instead, very far up beyond them all.
float floorHeight(Frame* frame, float2 xz, bool smooth = true, bool under = false)
{
    for (uint i = 0; i < FLOOR_SLICES; i++)
    {
        float4 slice = frame->ocean.floor[i];
        float2 uv = (xz - slice.xy) / slice.z;
        if (slice.w <= 0.0 || any(uv < 0.02) || any(uv > 0.98))
        {
            continue;
        }
        float height = floorSliceHeight(frame, i, uv, smooth, under);
        float edge = min(min(uv.x, uv.y), min(1.0 - uv.x, 1.0 - uv.y));
        float fade = saturate((0.15 - edge) / 0.12);
        if (fade > 0.0 && i + 1 < FLOOR_SLICES && frame->ocean.floor[i + 1].w > 0.0)
        {
            float4 next = frame->ocean.floor[i + 1];
            height = lerp(height, floorSliceHeight(frame, i + 1, (xz - next.xy) / next.z, smooth, under), fade);
        }
        return height;
    }
    return under ? 1e9 : -1e9;
}

// A corner of the mesh of Dry Area push.source, placed by its Obj.
[shader("vertex")]
float4 dryMeshVertex(uint vertexId : SV_VertexID) : SV_Position
{
    Frame* frame = push.frame;
    WaterBody body = buffers()->waterBodies[push.source];
    Mesh mesh = buffers()->meshes[body.liquid];
    Vertex v = buffers()->vertices[mesh.vertexOffset + vertexId];
    return transform(frame->viewProj, transform(body.toWorld, float4(v.position, 1.0)));
}

// Where a mesh Dry Area starts and ends along this pixel's view, kept by the
// greater of each (water::record_dry_mask): a face towards the camera as its
// depth in red, a face away from it as one less its depth in green.
[shader("fragment")]
float2 dryMeshFragment(float4 position : SV_Position, bool front : SV_IsFrontFace) : SV_Target0
{
    return front ? float2(position.z, 0.0) : float2(0.0, 1.0 - position.z);
}

// How deep the sea is round `xz` for how much its waves die away there: its
// depth averaged over a few metres (floorDepthMain), so the waves calm
// gradually where the ground drops away rather than in a step, and a rock or
// post in deep water barely calms them; from the finest slice that reaches
// it, fading into the next towards its edge. Deep beyond them all, and
// everywhere without Shores.
float seaDepth(Frame* frame, float2 xz)
{
    Texture2D depths = textures[NonUniformResourceIndex(frame->ocean.floorDepth)];
    SamplerState linear = samplers[SAMPLER_LINEAR_CLAMP];
    for (uint i = 0; i < FLOOR_SLICES; i++)
    {
        float4 slice = frame->ocean.floor[i];
        float2 uv = (xz - slice.xy) / slice.z;
        if (slice.w <= 0.0 || any(uv < 0.02) || any(uv > 0.98))
        {
            continue;
        }
        float depth = depths.SampleLevel(linear, (float2(float(i % 2), float(i / 2)) + uv) * 0.5, 0.0).r;
        float edge = min(min(uv.x, uv.y), min(1.0 - uv.x, 1.0 - uv.y));
        float fade = saturate((0.15 - edge) / 0.12);
        float4 next = i + 1 < FLOOR_SLICES ? frame->ocean.floor[i + 1] : 0.0;
        if (fade > 0.0 && next.w > 0.0)
        {
            float2 nextUv = (xz - next.xy) / next.z;
            float further = depths.SampleLevel(linear, (float2(float((i + 1) % 2), float((i + 1) / 2)) + nextUv) * 0.5, 0.0).r;
            depth = lerp(depth, further, fade);
        }
        return depth;
    }
    return SHALLOWS_DEPTH;
}

// The blur floorDepthMain spreads the sea's depth over, in metres, as a
// Gaussian's width; its taps reach twice as far.
static const float FLOOR_DEPTH_BLUR = 4.0;

// One texel of slice push.level of the sea floor's depth, blurred over a few
// metres for how much the waves calm (seaDepth): across, from the floor
// itself (push.texture) when push.textures.x is 0, then along, from what was
// blurred across. Depths are kept within SHALLOWS_DEPTH, past which waves
// do not calm, so a deep trench does not count for more than open sea.
[shader("compute")]
[numthreads(8, 8, 1)]
void floorDepthMain(uint3 id : SV_DispatchThreadID)
{
    if (any(id.xy >= FLOOR_TEXELS))
    {
        return;
    }
    Frame* frame = push.frame;
    uint slice = push.level;
    bool across = push.textures.x == 0;
    float texel = frame->ocean.floor[slice].z / float(FLOOR_TEXELS);
    float2 corner = float2(float(slice % 2), float(slice / 2)) * float(FLOOR_TEXELS);
    Texture2D from = textures[NonUniformResourceIndex(push.texture)];
    SamplerState linear = samplers[SAMPLER_LINEAR_CLAMP];
    float total = 0.0;
    float weights = 0.0;
    for (int k = -8; k <= 8; k++)
    {
        // A tap every quarter of the blur's width, within this slice.
        float offset = float(k) * FLOOR_DEPTH_BLUR * 0.25 / texel;
        float2 at = clamp(float2(id.xy) + 0.5 + (across ? float2(offset, 0.0) : float2(0.0, offset)), 0.5, float(FLOOR_TEXELS) - 0.5);
        float value = from.SampleLevel(linear, (corner + at) / float(across ? FLOOR_TEXELS * 4 : FLOOR_TEXELS * 2), 0.0).r;
        // Under an overhang, whose underside is over the sea, the water is as deep as if it were not there.
        float under = across ? from.SampleLevel(linear, (corner + at + float2(2 * FLOOR_TEXELS, 0.0)) / float(FLOOR_TEXELS * 4), 0.0).r : 0.0;
        float depth = across ? (under > 0.5 ? SHALLOWS_DEPTH : clamp(FLOOR_REACH - value * 2.0 * FLOOR_REACH, 0.0, SHALLOWS_DEPTH)) : value;
        float weight = exp(-0.5 * pow(float(k) * 0.25, 2.0));
        total += depth * weight;
        weights += weight;
    }
    storageImagesR32f[push.output][uint2(corner) + id.xy] = total / weights;
}

// How much of cascade `cascade`'s waves water `depth` metres deep keeps: a
// wave feels the bottom once the water is shallower than half its length,
// and dies away towards the shore, the long swell first (ocean::shallow_weight).
float shallowWeight(Frame* frame, uint cascade, float depth)
{
    float felt = min(2.0 * max(depth, 0.0) / (frame->ocean.sizes[cascade] * 0.1), 1.0);
    felt += (1.0 - felt) * saturate(depth / SHALLOWS_DEPTH);
    return 1.0 - 0.9 * (1.0 - felt);
}

// How much the waves move water sideways where it is `depth` metres deep:
// not at all at the very edge, so they cannot draw it up the shore
// (ocean::sideways_weight).
float sidewaysWeight(float depth)
{
    return saturate(depth);
}

// The thin line of foam where water `depth` metres deep meets the ground,
// worked out where it is drawn so it follows the ground finely. The surf
// beyond it is kept with the rest of the foam (oceanFoamMain), so it
// lingers and drains back.
float shoreEdge(float depth)
{
    return pow(saturate(1.0 - depth / 0.3), 2.0);
}

// How fast foam drifts downwind, in metres a second.
static const float FOAM_DRIFT = 0.25;

// Which way the wind blows over the sea, across the ground.
float2 windDirection(Frame* frame)
{
    return float2(cos(frame->ocean.wind), sin(frame->ocean.wind));
}

// ---------------------------------------------------------------- Foam's pattern

// The tiling pattern foam is made of: this many texels a side, covering
// this many metres of water.
static const uint FOAM_PATTERN = 1024;
static const float FOAM_PATTERN_SIZE = 6.0;

// The nearest and second nearest of a scatter of points a unit apart round
// `p`, repeating every `period` units: how far the nearest is, and how much
// further the second is (small along the edges between cells).
float2 tiledCells(float2 p, float period)
{
    float2 n = floor(p);
    float2 f = frac(p);
    float first = 8.0;
    float second = 8.0;
    for (int j = -1; j <= 1; j++)
    {
        for (int i = -1; i <= 1; i++)
        {
            float2 g = float2(i, j);
            float2 cell = n + g;
            float2 r = g + cellPoint(cell - floor(cell / period) * period) - f;
            float d = dot(r, r);
            if (d < first)
            {
                second = first;
                first = d;
            }
            else if (d < second)
            {
                second = d;
            }
        }
    }
    return float2(sqrt(first), sqrt(second) - sqrt(first));
}

// Smooth noise from 0 to 1 over `p`, repeating every `period` units: a
// slope at each whole point rather than a value, so its shapes do not line
// up along them where foam is cut from it.
float tiledNoise(float2 p, float period)
{
    float2 n = floor(p);
    float2 f = frac(p);
    float2 u = f * f * f * (f * (f * 6.0 - 15.0) + 10.0);
    float corner[4];
    for (uint i = 0; i < 4; i++)
    {
        float2 offset = float2(float(i & 1), float(i >> 1));
        float2 at = n + offset;
        corner[i] = dot(cellPoint(at - floor(at / period) * period) * 2.0 - 1.0, f - offset);
    }
    return saturate(0.5 + 1.2 * lerp(lerp(corner[0], corner[1], u.x), lerp(corner[2], corner[3], u.x), u.y));
}

// Smooth noise from 0 to 1 over the sea at `p`, a unit across, for foam.
float foamNoise(float2 p)
{
    return tiledNoise(p, 4096.0);
}

// Noise in three octaves over `p`, repeating every `period` units.
float tiledBillows(float2 p, float period)
{
    return tiledNoise(p, period) * 0.5 + tiledNoise(p * 2.0, period * 2.0) * 0.3 + tiledNoise(p * 4.0, period * 4.0) * 0.2;
}

// One texel of foam's pattern (storage image push.output): in red, fresh
// froth, highest where its bubbles clump, lowest in the holes between; in
// green, small bubbles, for under the surface; in blue, the rounded tops of
// the bubbles the froth is made of, 1 at a bubble's top; in alpha, the
// streaks old foam thins to.
[shader("compute")]
[numthreads(8, 8, 1)]
void foamPatternMain(uint3 id : SV_DispatchThreadID)
{
    float2 uv = (float2(id.xy) + 0.5) / float(FOAM_PATTERN);
    // Bent, so nothing in it lines up.
    float2 warp = float2(tiledBillows(uv * 4.0, 4.0), tiledBillows(uv * 4.0 + 0.37, 4.0)) - 0.5;
    float2 bent = uv + warp * 0.12;
    float body = tiledBillows(bent * 5.0, 5.0);
    float fine = tiledBillows(bent * 19.0, 19.0);
    float2 tiny = tiledCells(bent * 160.0, 160.0);
    float domes = sqrt(saturate(1.0 - pow(tiny.x / 0.5, 2.0)));
    // Fresh foam: clumps of small bubbles, with holes between.
    float froth = saturate((body * 0.75 + fine * 0.35 - 0.32) * 2.2) * (0.88 + 0.12 * domes);
    // Old foam: thin bubbly streaks drawn out by the water, and a few patches yet to burst.
    float2 drawn = uv + warp * 0.25;
    float streaks = max((1.0 - smoothstep(0.0, 0.03, abs(tiledBillows(drawn * 4.0, 4.0) - 0.5))) * 0.95,
        (1.0 - smoothstep(0.0, 0.025, abs(tiledBillows(drawn * 9.0 + 0.5, 9.0) - 0.5))) * 0.4);
    // Broken along their length, so they do not run on as unbroken threads.
    streaks *= smoothstep(0.38, 0.62, tiledBillows(drawn * 14.0 + 0.2, 14.0));
    float patches = smoothstep(0.62, 0.72, body);
    float lace = saturate(streaks * (0.5 + 0.5 * fine) * (0.8 + 0.2 * domes) + patches * (0.9 + 0.1 * domes) * 0.8);
    // Bubbles under the surface, small and scattered.
    float2 round = tiledCells(bent * 30.0 + 0.37, 30.0);
    float bubbles = (1.0 - smoothstep(0.05, 0.35, round.x)) * smoothstep(0.4, 0.7, tiledNoise(uv * 9.0, 9.0));
    storageImagesRgba16f[push.output][id.xy] = float4(froth, bubbles, domes, lace);
}

// Foam's pattern over the water resting at `rest`: two sizes of it laid
// over each other, so it does not visibly repeat, filtered by distance; at
// least `blur` mips softer. With `drifting`, the smaller drifts downwind
// with the foam the sea keeps, and the larger, turned, more slowly and a
// little across it, so the two never lie the same way over each other twice.
float4 foamPatternAt(Frame* frame, float2 rest, float blur = 0.0, bool drifting = true)
{
    Texture2D pattern = textures[NonUniformResourceIndex(frame->ocean.foamPattern)];
    SamplerState repeat = samplers[SAMPLER_LINEAR_REPEAT];
    float2 wind = drifting ? windDirection(frame) * FOAM_DRIFT * frame->time : 0.0;
    float2 a = (rest - wind) / FOAM_PATTERN_SIZE;
    float2 turned = float2(dot(rest, float2(0.8, -0.6)), dot(rest, float2(0.6, 0.8)));
    float2 b = (turned - float2(wind.y, wind.x) * 0.6) / (FOAM_PATTERN_SIZE * 2.7) + 0.31;
    float4 near = blur > 0.0 ? pattern.SampleLevel(repeat, a, blur) : pattern.Sample(repeat, a);
    float4 far = blur > 0.0 ? pattern.SampleLevel(repeat, b, blur) : pattern.Sample(repeat, b);
    return near * 0.65 + far * 0.35;
}

// Where on foam's pattern a pixel of the sea reads, and how old its foam
// is. The pattern rides the waves but is not carried along by the surf:
// that runs out from a rock on every side and back, and would stretch it
// round the rock.
struct FoamSpot
{
    float2 shift;
    float age;
};

// Foam's pattern at `at` for `spot`: dense froth while it is fresh, lace as
// it ages; `domes`, how its bubbles round up, flatter on old foam.
float frothPattern(Frame* frame, float2 at, FoamSpot spot, out float domes)
{
    float4 pattern = foamPatternAt(frame, at + spot.shift);
    float aged = smoothstep(0.25, 0.75, spot.age);
    domes = lerp(pattern.z, 0.3, aged);
    return lerp(pattern.x, pattern.w, aged);
}

// Foam seen as a layer of froth standing on the water: where the view
// meets it, how it leans there, how much sun reaches it past the clumps
// between, and how far down between them it is.
struct Froth
{
    float amount;
    float3 tilt;
    float sun;
    float crevice;
    float glint;
};

// How much froth stands at `at` on the pattern, from 0 to 1 of its layer:
// where there is foam, the rounded tops of its bubbles, sloping gently down
// to the water at its edges.
float frothHeight(Frame* frame, float2 at, float from, float edge, float coverage, FoamSpot spot)
{
    float domes;
    float pattern = frothPattern(frame, at, spot, domes);
    float there = smoothstep(from, from + edge * 2.0, pattern) * saturate(coverage * 4.0);
    return there * (0.85 + 0.15 * domes);
}

// The froth over the water at `rest` (on the foam's pattern), seen along
// `v`, lit from `l`: a layer up to a few centimetres thick, thicker the more
// foam there is and thinning out with distance by `near`, its height the
// pattern revealed as `from` and `edge` reveal it. The view is marched down
// through the layer to where it first meets froth, then towards the sun to
// see what clumps shade it.
Froth frothAt(Frame* frame, float2 rest, float3 v, float3 l, float coverage, float from, float edge, float near, FoamSpot spot)
{
    static const uint SHADOW_STEPS = 4;
    // More steps the more the view slants across the layer, so its sides do not break into slices.
    uint steps = uint(lerp(8.0, 24.0, saturate(1.0 - v.y * 1.5)));
    Froth froth;
    froth.amount = 0.0;
    froth.tilt = 0.0;
    froth.sun = 1.0;
    froth.crevice = 1.0;
    froth.glint = 0.0;
    // Old foam lies flat on the water.
    float thick = 0.03 * (0.5 + coverage) * (1.0 - 0.9 * spot.age) * near;
    // Where the view crosses the layer's top, over the water this pixel shows.
    float2 across = v.xz / max(v.y, 0.2) * thick;
    float before = 1.0;
    float beforeGap = frothHeight(frame, rest + across, from, edge, coverage, spot) - 1.0;
    float hit = -1.0;
    for (uint i = 1; i <= steps; i++)
    {
        float height = 1.0 - float(i) / float(steps);
        float gap = frothHeight(frame, rest + across * height, from, edge, coverage, spot) - height;
        if (gap >= 0.0)
        {
            hit = lerp(before, height, beforeGap / min(beforeGap - gap, -1e-4));
            break;
        }
        before = height;
        beforeGap = gap;
    }
    if (hit < 0.0)
    {
        return froth;
    }
    float2 at = rest + across * hit;
    // Thin where it barely stands off the water, the water showing through.
    froth.amount = smoothstep(0.0, 0.3, hit);
    // How it leans, from how its height changes either side.
    float e = 0.012;
    float dx = frothHeight(frame, at + float2(e, 0.0), from, edge, coverage, spot) - frothHeight(frame, at - float2(e, 0.0), from, edge, coverage, spot);
    float dz = frothHeight(frame, at + float2(0.0, e), from, edge, coverage, spot) - frothHeight(frame, at - float2(0.0, e), from, edge, coverage, spot);
    froth.tilt = float3(-dx, 0.0, -dz) * thick / (2.0 * e);
    // Clumps between it and the sun shade it, softly.
    float2 sunward = l.xz / max(l.y, 0.2) * thick;
    float shade = 0.0;
    for (uint j = 1; j <= SHADOW_STEPS; j++)
    {
        float height = hit + (1.0 - hit) * float(j) / float(SHADOW_STEPS);
        shade += saturate((frothHeight(frame, at + sunward * (height - hit), from, edge, coverage, spot) - height) * 4.0);
    }
    froth.sun = 1.0 - 0.75 * saturate(shade / float(SHADOW_STEPS) * 2.0);
    froth.crevice = 0.55 + 0.45 * hit;
    // A wet glint off each bubble's top.
    float3 bubbleNormal = normalize(float3(0.0, 1.0, 0.0) + froth.tilt);
    froth.glint = pow(saturate(dot(reflect(-v, bubbleNormal), l)), 80.0) * froth.sun * (1.0 - spot.age);
    return froth;
}

// How much foam a crest breaking at `rest` leaves behind: only where the
// water folds well past where crest foam begins, so the big breakers trail
// foam and the small ones' white is gone as they pass.
float breakingAt(Frame* frame, float2 rest, float depth)
{
    float folded = 1.0;
    for (uint c = 0; c < OCEAN_BREAKING_CASCADES; c++)
    {
        float4 d = textures[NonUniformResourceIndex(frame->ocean.displacement[c])].SampleLevel(samplers[SAMPLER_LINEAR_REPEAT], cascadeUv(frame, c, rest), 1.0);
        folded += (d.w - 1.0) * shallowWeight(frame, c, depth);
    }
    float threshold = 0.1 + 0.45 * frame->ocean.foam;
    return frame->ocean.foam > 0.0 ? saturate((threshold - folded) / 0.3) : 0.0;
}

// How hard the Streams running into the sea churn it at `xz`: white where
// each pours in, the point a Stream's water body's size.z names, carried on
// out in a jet as wide as it, reaching further the faster it runs.
float pouredIn(Frame* frame, float2 xz)
{
    float churned = 0.0;
    for (uint i = frame->ocean.pools; i < frame->ocean.pools + frame->ocean.streams; i++)
    {
        WaterBody body = buffers()->waterBodies[i];
        if (body.liquid != LIQUID_WATER || body.size.z < 0.0)
        {
            continue;
        }
        StreamPoint end = buffers()->streamPoints[body.shape + uint(body.size.z)];
        if (abs(end.meets - frame->ocean.level) > 1e-3)
        {
            continue;
        }
        float2 along = normalize(end.tangent.xz + float2(0.0, 1e-5));
        float2 off = xz - end.position.xz;
        float ahead = dot(off, along);
        float aside = abs(dot(off, float2(-along.y, along.x)));
        float reach = 1.0 + end.speed * 2.5;
        float jet = saturate(1.0 - aside / (end.width * 0.5 + max(ahead, 0.0) * 0.3)) * saturate(1.0 + ahead) * saturate(1.0 - ahead / reach);
        churned += jet * saturate(end.speed / 3.0);
    }
    return churned;
}

// One texel of the foam round the camera (storage image push.output), kept
// by where the water rests, as the surface is drawn, so it rides the waves'
// orbits as the water does and stays behind as a breaking crest runs on: a
// streak down the wave's back. In red, how much foam there is; in green,
// how much of it is fresh, the rest ageing towards lace. It is made where
// crests break and where the water surges against the ground, drifts
// downwind, runs up the shore and drains back with each wave, spreads and
// fades. Last
// frame's foam in push.textures.x, its first texel's corner in the bits of
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
    float dt = clamp(frame->deltaTime, 1e-4, 0.1);
    // Where the waves have carried the water resting here, and how fast it is rising.
    float restingDepth = seaDepth(frame, here);
    float3 moved = oceanMoved(frame, here, FOAM_TEXEL, false, 1.0, restingDepth);
    float rising = (moved.y - oceanMoved(frame, here, FOAM_TEXEL, true, 1.0, restingDepth).y) / dt;
    float2 at = here + moved.xz;
    float waterY = frame->ocean.level + moved.y;
    float ground = groundUnder(frame, at, waterY, false);
    float depth = waterY - ground;
    // Ground standing out of the water a stride or so round, as round a rock or up a beach, and which way it rises.
    float land = 0.0;
    float2 uphill = 0.0;
    for (uint k = 0; k < 8; k++)
    {
        float2 around = float2(cos(float(k) * PI / 4.0), sin(float(k) * PI / 4.0));
        float there = groundUnder(frame, at + around, waterY, false);
        land += saturate((there - waterY + 0.3) / 0.6);
        uphill += around * clamp(there - ground, -2.0, 2.0);
    }
    // Fading out where the ground stands out of the water, rather than stopping on the edge of a texel.
    float wet = smoothstep(-0.5, -0.1, depth);
    float contact = wet * max(saturate(1.0 - depth / 0.5), saturate(land / 8.0 * 2.5));
    // Carried downwind, and near land up towards it as a wave comes in, and back more slowly as it drains.
    float2 towards = length(uphill) > 1e-3 ? normalize(uphill) : 0.0;
    float2 flow = windDirection(frame) * FOAM_DRIFT + towards * clamp(rising * (rising > 0.0 ? 1.2 : 0.4), -1.5, 1.5) * contact;

    float4 kept = 0.0;
    float2 previousOrigin = float2(asfloat(push.textures.y), asfloat(push.textures.z));
    float2 uv = (here - flow * dt - previousOrigin) / (float(FOAM_TEXELS) * FOAM_TEXEL);
    if (!isnan(previousOrigin.x) && all(uv > 0.0) && all(uv < 1.0))
    {
        Texture2D before = textures[NonUniformResourceIndex(push.textures.x)];
        SamplerState clamped = samplers[SAMPLER_LINEAR_CLAMP];
        float nudge = 0.5 / float(FOAM_TEXELS);
        float4 round = before.SampleLevel(clamped, uv + float2(nudge, nudge), 0.0) + before.SampleLevel(clamped, uv - float2(nudge, nudge), 0.0)
            + before.SampleLevel(clamped, uv + float2(nudge, -nudge), 0.0) + before.SampleLevel(clamped, uv + float2(-nudge, nudge), 0.0);
        // Spreading out a little as it goes.
        kept = lerp(before.SampleLevel(clamped, uv, 0.0), round * 0.25, 1.0 - exp(-dt * 4.0));
    }
    // Crests make it as they break; water churning against the ground, each place a little differently, fresh as
    // it surges up, the more the faster it comes. Fresh foam ages into lace in a second or so; all of it is gone in
    // several.
    float breaking = breakingAt(frame, here, restingDepth);
    // Waves break as they reach water not much deeper than they are high: the crests the open sea would bring here,
    // before the shallows calm them, tumble into surf down their rising fronts, in stretches along the shore.
    if (restingDepth > 0.0 && restingDepth < 6.0)
    {
        float crest = oceanMoved(frame, here, FOAM_TEXEL, false).y;
        float front = saturate((crest - oceanMoved(frame, here, FOAM_TEXEL, true).y) / dt * 2.0);
        float tall = saturate((crest - 0.45 * restingDepth) / (0.3 * restingDepth + 0.1));
        breaking = max(breaking, tall * front * smoothstep(0.3, 0.7, foamNoise(here * 0.25 + frame->time * 0.05)));
    }
    float churn = contact * (0.2 + 1.4 * foamNoise(here * 0.4 + frame->time * 0.1)) * 3.0 * dt;
    float surging = saturate(rising * 3.0);
    float poured = pouredIn(frame, here) * 2.0 * dt;
    float amount = saturate(max(kept.r * exp(-dt / 6.0), breaking) + churn * (0.1 + surging) + poured);
    float fresh = min(saturate(max(kept.g * exp(-dt / 1.0), breaking) + churn * surging + poured), amount);
    storageImagesRgba16f[push.output][id.xy] = float4(amount * wet, fresh * wet, 0.0, 0.0);
}

// The foam round the camera at `rest` (oceanFoamMain): how much, and how
// much of it fresh; none outside the square it keeps.
float4 keptFoam(Frame* frame, float2 rest)
{
    float2 uv = (rest - frame->ocean.foamOrigin) / (float(FOAM_TEXELS) * FOAM_TEXEL);
    if (!oceanHas(frame, OCEAN_FOAM_TRAILS) || any(uv <= 0.0) || any(uv >= 1.0))
    {
        return 0.0;
    }
    // Smoothly, so its texels' edges do not show where foam is revealed from it.
    return smoothSample(textures[NonUniformResourceIndex(frame->ocean.foamTexture)], uv, float(FOAM_TEXELS));
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
    // A Stream's: how far into its mouth it is, where it runs into the sea or a Pool, from 0 upstream to 1.
    float mouth : MOUTH;
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
// as `swell` says; in water `depth` metres deep, the waves dying away in
// the shallows.
float3 oceanMoved(Frame* frame, float2 rest, float spacing, bool previous, float swell = 1.0, float depth = 1e4)
{
    float3 moved = 0.0;
    for (uint c = 0; c < oceanCascades(frame); c++)
    {
        uint image = previous ? frame->ocean.previous[c] : frame->ocean.displacement[c];
        float mip = max(log2(spacing / (frame->ocean.sizes[c] / float(OCEAN_N))), 0.0);
        float3 d = textures[NonUniformResourceIndex(image)].SampleLevel(samplers[SAMPLER_LINEAR_REPEAT], cascadeUv(frame, c, rest), mip).xyz;
        moved += d * cascadeShare(c, swell) * shallowWeight(frame, c, depth);
    }
    moved.xz *= sidewaysWeight(depth);
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
    float depth = seaDepth(frame, rest);
    float3 world = float3(rest.x, level, rest.y) + oceanMoved(frame, rest, sampled, false, 1.0, depth);
    float3 before = float3(rest.x, level, rest.y) + oceanMoved(frame, rest, sampled, true, 1.0, depth);
    o.position = transform(frame->viewProj, float4(world, 1.0));
    o.world = world;
    o.rest = rest;
    o.currentClip = transform(frame->unjitteredViewProj, float4(world, 1.0));
    o.previousClip = transform(frame->prevViewProj, float4(before, 1.0));
    o.body = SEA;
    o.stream = 0.0;
    o.lava = 0.0;
    o.mouth = 0.0;
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
    o.mouth = 0.0;
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
    float spacing = poolSpacing(body);
    // Lava is too thick for waves.
    float wet = body.liquid == LIQUID_LAVA ? 0.0 : 1.0;
    float3 world = poolMoved(body, across, oceanMoved(frame, flat.xz, spacing, false, body.waves) * wet);
    float3 before = poolMoved(body, across, oceanMoved(frame, flat.xz, spacing, true, body.waves) * wet);
    WaterOut o;
    o.position = transform(frame->viewProj, float4(world, 1.0));
    o.world = world;
    o.rest = flat.xz;
    o.currentClip = transform(frame->unjitteredViewProj, float4(world, 1.0));
    o.previousClip = transform(frame->prevViewProj, float4(before, 1.0));
    o.body = pool;
    o.stream = 0.0;
    o.lava = 0.0;
    o.mouth = 0.0;
    o.up = float3(0.0, 1.0, 0.0);
    return o;
}

// Where the water of Pool `pool` resting `across` its footprint is moved to
// by its waves, `moved`: up and down in full, but sideways less and less
// towards its sides and never past them, so its surface stays inside its
// walls however high the waves.
float3 poolMoved(WaterBody pool, float2 across, float3 moved)
{
    float3 local = transform(pool.toLocal, float4(moved, 0.0)).xyz;
    float2 half = pool.size.xz * 0.5;
    float2 room = half - abs(across);
    float2 held = across + local.xz * saturate(min(room.x, room.y) / max(0.15 * min(pool.size.x, pool.size.z), 0.01));
    held = clamp(held, -half, half);
    return transform(pool.toWorld, float4(held.x, local.y, held.y, 1.0)).xyz;
}

// How far apart a Pool's grid's points are, in metres, which its walls read
// its waves as coarsely as, to meet its surface.
float poolSpacing(WaterBody pool)
{
    return max(pool.size.x, pool.size.z) / float(POOL_QUADS);
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
    // At its mouth it eases to the height of the water it runs into, rising and falling with the sea's waves there.
    if (p.mouth > 0.0)
    {
        bool sea = frame->ocean.enabled != 0 && abs(p.meets - frame->ocean.level) < 1e-3;
        float waves = sea ? oceanMoved(frame, world.xz, 0.5, false, 1.0, seaDepth(frame, world.xz)).y : 0.0;
        world.y = lerp(world.y, p.meets + waves, p.mouth);
    }
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
    o.mouth = p.mouth;
    return o;
}

// An image `texels` across (a cascade's, unless told) at `uv` through a
// cubic B-spline over its 4×4 nearest texels, made of four bilinear reads:
// smooth in value and slope, so a texel's edges do not show however near
// it is.
float4 smoothSample(Texture2D image, float2 uv, float texels = float(OCEAN_N))
{
    float2 texel = uv * texels - 0.5;
    float2 f = frac(texel);
    float2 i = floor(texel);
    float2 f2 = f * f;
    float2 f3 = f2 * f;
    float2 w0 = (1.0 - 3.0 * f + 3.0 * f2 - f3) / 6.0;
    float2 w1 = (4.0 - 6.0 * f2 + 3.0 * f3) / 6.0;
    float2 w2 = (1.0 + 3.0 * f + 3.0 * f2 - 3.0 * f3) / 6.0;
    float2 w3 = f3 / 6.0;
    float2 s0 = w0 + w1;
    float2 s1 = w2 + w3;
    // Each pair of texels as one bilinear read between them, at their centres' weighted middle.
    float2 p0 = (i - 1.0 + w1 / s0 + 0.5) / texels;
    float2 p1 = (i + 1.0 + w3 / s1 + 0.5) / texels;
    SamplerState linear = samplers[SAMPLER_LINEAR_REPEAT];
    return s0.y * (s0.x * image.SampleLevel(linear, float2(p0.x, p0.y), 0.0) + s1.x * image.SampleLevel(linear, float2(p1.x, p0.y), 0.0))
         + s1.y * (s0.x * image.SampleLevel(linear, float2(p0.x, p1.y), 0.0) + s1.x * image.SampleLevel(linear, float2(p1.x, p1.y), 0.0));
}

// The waves at `rest`, with as much of the sea's long swell as `swell`
// says, added in by `weight`: their slopes, the slopes too small to see
// there as variance, how much they fold and how far they move the water.
void addWaves(Frame* frame, float2 rest, float swell, float weight, inout float2 slope, inout float variance, inout float folded, inout float3 moved, float depth = 1e4)
{
    for (uint c = 0; c < oceanCascades(frame); c++)
    {
        float2 uv = cascadeUv(frame, c, rest);
        // How many of the cascade's texels a pixel spans: under one, they are seen magnified.
        float2 texels = uv * float(OCEAN_N);
        float footprint = max(length(ddx(texels)), length(ddy(texels)));
        Texture2D slopes = textures[NonUniformResourceIndex(frame->ocean.slopes[c])];
        Texture2D moves = textures[NonUniformResourceIndex(frame->ocean.displacement[c])];
        float4 s = 0.0;
        float4 d = 0.0;
        // Magnified, a bilinear texel's kinks at its edges show in sharp reflections as a grid of squares, so a
        // smooth cubic takes over, blended in so no line shows where it starts.
        float smooth = 1.0 - saturate((footprint - 0.5) * 2.0);
        if (smooth > 0.0)
        {
            s = smoothSample(slopes, uv) * smooth;
            d = smoothSample(moves, uv) * smooth;
        }
        if (smooth < 1.0)
        {
            s += slopes.Sample(samplers[SAMPLER_LINEAR_REPEAT], uv) * (1.0 - smooth);
            d += moves.Sample(samplers[SAMPLER_LINEAR_REPEAT], uv) * (1.0 - smooth);
        }
        float share = cascadeShare(c, swell) * weight * shallowWeight(frame, c, depth);
        slope += s.xy * share;
        // The slopes the mips averaged away, too small to see here, as roughness. Magnified, filtering
        // the squares apart from the slopes would make roughness of nothing, so it starts only as texels shrink
        // under a pixel.
        float2 squares = lerp(s.xy * s.xy, s.zw, saturate(0.25 * log2(max(footprint, 1.0))));
        variance += (max(squares.x - s.x * s.x, 0.0) + max(squares.y - s.y * s.y, 0.0)) * share * share;
        folded += c < OCEAN_BREAKING_CASCADES ? (d.w - 1.0) * share : 0.0;
        moved += d.xyz * share * float3(sidewaysWeight(depth), 1.0, sidewaysWeight(depth));
    }
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

// Whether water drawn at `world` is kept out: inside a Dry Area, whatever
// its shape; and, for
// the sea, over a Pool lower than it, which is dry to the sea.
bool keptDry(Frame* frame, float3 world, uint body)
{
    if (inDryMesh(frame, world))
    {
        return true;
    }
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
        surface += oceanMoved(frame, camera.xz, 0.05, false, 1.0, seaDepth(frame, camera.xz)).y;
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
// the thin top of a wave; the
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
    // The sea stops at the shore: none where the ground stands above it, but on under an overhang.
    float ground = sea ? groundUnder(frame, input.world.xz, input.world.y) : -1e9;
    if (ground > input.world.y)
    {
        discard;
    }
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
    // A Stream's ripples drift down it in two phases: two sets, each carried along for a period
    // and then started again, half a period apart, each faded out as it starts again.
    float2 flow = input.stream.xy;
    float phase = frac(frame->ocean.time / FLOW_PERIOD);
    float2 restA = rest - flow * phase * FLOW_PERIOD;
    float2 restB = rest - flow * frac(phase + 0.5) * FLOW_PERIOD;
    float blend = abs(1.0 - 2.0 * phase);
    if (stream)
    {
        // Its own ripples, carried along, giving way at its mouth to the waves of the water it runs into.
        float own = 1.0 - input.mouth;
        addWaves(frame, restA, 0.0, (1.0 - blend) * own, slope, variance, folded, moved);
        addWaves(frame, restB, 0.0, blend * own, slope, variance, folded, moved);
        // Faster water is rougher.
        slope *= 0.6 + 0.25 * length(flow) * own;
        if (input.mouth > 0.0)
        {
            addWaves(frame, rest, 1.0, input.mouth, slope, variance, folded, moved);
        }
    }
    else
    {
        addWaves(frame, rest, waves, 1.0, slope, variance, folded, moved, sea ? seaDepth(frame, input.world.xz) : 1e4);
    }
    float3 n = normalize(float3(-slope.x, 0.0, -slope.y) + input.up);
    float3 v = normalize(frame->cameraPosition - input.world);
    // From under the water the surface shows its underside; from the air, a wave above the camera its front. Which
    // it is goes by where this pixel's view starts, so with the camera half in, each half sees its own side.
    float3 near;
    float level;
    float3 absorbed;
    if (waterlineAt(frame, input.position.xy, near, level, absorbed) < 0.0)
    {
        return fromBelow(frame, input, n, v, deep, scatter, absorb);
    }
    float3 l = -frame->sunDirection;
    bool atmosphere = frame->lighting.sky == SKY_ATMOSPHERE;
    float3 sunLight = atmosphere ? sunThroughAtmosphere(frame) : frame->sunColor;
    float shadow = sunShadow(frame, input.world, float3(0.0, 1.0, 0.0));
    // Rougher where the surface's normal turns fast across a pixel, so glints there spread rather than sparkle a
    // pixel at a time.
    float3 nx = ddx(n);
    float3 ny = ddy(n);
    float turning = min(2.0 * 0.159 * (dot(nx, nx) + dot(ny, ny)), 0.18);
    float alpha = sqrt(0.02 * 0.02 + 2.0 * variance + turning);

    Surface s;
    s.normal = n;
    s.view = v;
    s.roughness = clamp(sqrt(alpha), 0.03, 1.0);
    s.diffuse = 0.0;
    s.f0 = 0.02;
    // Fresnel over the roughness's spread of slopes, so a rough sea's horizon is not over-bright.
    float nv = saturate(dot(n, v));
    float fresnel = 0.02 + 0.98 * pow(1.0 - nv, 5.0 * exp(-2.69 * alpha)) / (1.0 + 22.7 * pow(alpha, 1.5));
    float3 ambient = ambientLight(frame, input.world, s, sunLight, atmosphere);
    float3 reflected = surroundingsReflection(frame, input.world, n, reflect(-v, n), s.roughness, ambient, atmosphere);
    // The sun, a little rougher than the water so its disc shows as a streak of glints rather than a point.
    Surface sunSurface = s;
    sunSurface.roughness = max(s.roughness, 0.08);
    float3 glints = shade(sunSurface, l, sunLight) * shadow;

    // The water's own colour: deep blue lit by the sky, and the scatter colour where the
    // sun shines through the thin top of a wave towards the eye.
    float height = max(frame->ocean.waveHeight * max(waves, 0.2), 0.05);
    // Where the waves pinch the water most sideways, the crests are thinnest and
    // let most light through; a little on every high wave too. A Stream's ripples have no crests to glow.
    float pinched = saturate(length(moved.xz) / (height * 0.35));
    float crest = stream ? 0.0 : max(pinched * pinched, saturate(moved.y / (height * 0.5)) * 0.5);
    float towards = pow(saturate(dot(l, -v)), 4.0) * pow(saturate(0.5 - 0.5 * dot(l, n)), 3.0);
    // Crests glow through: strongly towards the sun, and a little from every side on a wave's top.
    float3 throughWave = (3.0 * crest * towards + 0.12 * crest * pow(saturate(dot(v, n)), 2.0) + 0.04 * saturate(dot(l, n))) * scatter * sunLight * shadow;
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
    // Light bends as it goes into the water, by Snell's law with water's index of 1.33: what shows under a pixel
    // lies along the bent ray, as far into the water as the view goes there. Only what is really behind the water
    // shows from where the ray bends to, not something standing in front of it.
    float2 seenAt = pixel / float2(frame->viewport);
    if (oceanHas(frame, OCEAN_LIGHT_BENDING) && thickness < 1e3)
    {
        float3 under = input.world + refract(-v, n, 1.0 / 1.33) * min(thickness, 10.0);
        float4 clip = transform(frame->viewProj, float4(under, 1.0));
        float2 bent = clip.xy / clip.w * 0.5 + 0.5;
        if (clip.w > 0.0 && all(bent > 0.0) && all(bent < 1.0))
        {
            float there = textures[NonUniformResourceIndex(push.textures.y)].Load(int3(int2(bent * float2(frame->viewport)), 0)).r;
            seenAt = (there > 0.0 ? frame->near / there : 1e4) > surfaceDepth ? bent : seenAt;
        }
    }
    // As sharp as clear water shows what is under it, blurring only where it is deep or rough.
    float blur = sqrt(s.roughness) * 1.2 + log2(1.0 + thickness * 0.12);
    float3 behind = thickness < 1e3 ? glassBehind(push.texture, seenAt, blur) : 0.0;
    // How much foam there is: where a crest folds over, and where the water thins at a shore. Only where the water
    // folds hard does a crest break: a calm sea's gentle folding stays clear.
    float threshold = 0.4 + 0.45 * frame->ocean.foam;
    float breaking = frame->ocean.foam > 0.0 ? saturate((threshold - folded) / 0.3) : 0.0;
    // A line of foam where the water meets the ground, by its true depth under the moving surface, worked out from
    // the waves here, not the mesh's, whose rings each carry the ripples a little differently; by how far the view
    // goes through the water where the ground is not known.
    float waterY = frame->ocean.level + moved.y;
    float shore = ground > -1e8 ? shoreEdge(waterY - ground) : saturate(1.0 - thickness / 0.6);
    // What breaking crests and the surf left, carried with the water (the sea's only). Some stretches of sea hold
    // their foam longer than others, the patches shifting slowly.
    float4 trail = sea ? keptFoam(frame, rest) : 0.0;
    float patchy = 0.6 + 0.8 * foamNoise(rest / 40.0 + frame->time * 0.02);
    float fresh = max(max(breaking, shore), trail.g * 1.2 * patchy);
    // Where on the foam's pattern: riding the waves; down a Stream, drawn out along it and carried with it.
    FoamSpot spot;
    spot.shift = moved.xz * 0.3;
    spot.age = 0.0;
    float2 bubbleAt = rest - v.xz / max(v.y, 0.2) * 0.25;
    float streamPattern = 0.0;
    if (stream)
    {
        float speed = length(flow);
        float2 along = flow / max(speed, 1e-4);
        float2 across = float2(-along.y, along.x);
        float2 a = float2(dot(restA, along) * 0.35, dot(restA, across));
        float2 b = float2(dot(restB, along) * 0.35, dot(restB, across));
        breaking = max(input.stream.z, 0.3 * saturate(speed / 4.0));
        streamPattern = lerp(foamPatternAt(frame, a, 0.0, false).x, foamPatternAt(frame, b, 0.0, false).x, blend);
        fresh = 1.0;
    }
    float coverage = saturate(max(max(breaking, shore), trail.r * 1.2 * patchy));
    // Old foam breaks up into islands as its bubbles burst, rather than thinning evenly.
    spot.age = coverage > 0.0 ? 1.0 - saturate(fresh / coverage) : 0.0;
    coverage *= 1.0 - spot.age * 0.6 * smoothstep(0.35, 0.65, foamNoise(rest * 0.6 + frame->time * 0.05));
    float domes = 0.0;
    float pattern = stream ? streamPattern : frothPattern(frame, rest, spot, domes);
    // Bubbles a hand's breadth under the foam, seen through the water, a pale turquoise, more as it ages.
    float bubbles = coverage > 0.1 ? smoothstep(0.1, 0.5, coverage) * foamPatternAt(frame, bubbleAt, 4.0).y * (0.3 + 0.4 * spot.age) : 0.0;
    float3 bubbleLit = (scatter * 0.6 + 0.2) * (ambient + sunLight * shadow * 0.25);
    float3 under = behind * through + body * (1.0 - through) + bubbleLit * bubbles * 0.25;
    float3 color = (1.0 - fresnel) * under + fresnel * reflected + glints;
    // Thinning to nothing where it meets the ground, rather than ending in a hard bright edge.
    color = lerp(glassBehind(push.texture, pixel / float2(frame->viewport), 0.0), color, saturate(thickness / 0.1));
    // The foam itself: as much of the pattern as there is foam, its thickest froth first, so a little foam is a
    // lace of its densest threads and the most, white water, is all of it. Its edge never sharper than a pixel or two.
    float edge = max(0.3 + 0.3 * spot.age, fwidth(pattern) * 1.5);
    // Even white water keeps its froth's deepest hollows.
    float from = max(1.0 - coverage * 1.8, 0.03);
    float foam = smoothstep(from, from + edge, pattern) * saturate(coverage * 4.0);
    float3 foamNormal = bumped(input.world, n, pattern * 0.03);
    float foamSun = 1.0;
    float crevice = 1.0;
    float glint = 0.0;
    // Near the camera it stands up off the water as a layer of froth, its clumps hiding what is behind them and
    // shading each other; further off, where a clump is smaller than a pixel, it lies flat.
    float closeness = saturate(1.0 - length(input.world - frame->cameraPosition) / 40.0);
    if (sea && coverage > 0.01 && closeness > 0.0 && oceanHas(frame, OCEAN_FOAM_3D))
    {
        Froth froth = frothAt(frame, rest, v, l, coverage, from, edge, closeness, spot);
        // Settling flat over its last few metres, so no line shows where it stops standing up.
        float standing = smoothstep(0.0, 0.25, closeness);
        foam = lerp(foam, froth.amount, standing);
        foamNormal = normalize(lerp(foamNormal, normalize(n + froth.tilt), standing));
        foamSun = lerp(1.0, froth.sun, standing);
        crevice = lerp(1.0, froth.crevice, standing);
        glint = froth.glint * standing;
    }
    // Lit as a rough white surface, light wrapping round its bubbles, darker down between its clumps.
    float wrapped = saturate((dot(foamNormal, l) + 0.4) / 1.4);
    // Thick fresh foam is near white; thin old lace is greyer, the water's own colour coming through it.
    float density = stream ? 1.0 : saturate(coverage * 1.5) * (1.0 - 0.5 * spot.age);
    float3 tint = lerp(1.0, scatter / max(max(max(scatter.r, scatter.g), scatter.b), 1e-3), 0.25);
    // Brighter on its densest froth and speckled by its bubbles, each brighter at its rim; a Stream's, drawn out
    // along it, is too fine to show either.
    float speckle = stream ? 1.0 : (0.8 + 0.25 * saturate(pattern)) * (0.93 + 0.07 * domes);
    float3 foamColor = 0.9 * lerp(tint * 0.7, 1.0, density) * speckle;
    float3 foamLit = foamColor * crevice * (ambient + sunLight * shadow * foamSun * wrapped / PI) + sunLight * shadow * glint * 0.6;
    float sparse = stream ? 1.0 : lerp(0.75, 1.0, saturate(pattern * 1.4));
    color = lerp(color, foamLit, foam * (1.0 - 0.35 * spot.age) * sparse);
    // A Stream fades in over its first two points and out over its last two, and across its mouth into the water it
    // runs into, drawn under it, so it becomes that water rather than lying on it.
    if (stream)
    {
        // Its banks first, so no edge of it shows on the water.
        float into = input.mouth > 0.0 ? saturate(input.mouth * (1.0 + 3.0 * input.lava.y * input.lava.y) + 0.05) : 0.0;
        float shown = saturate(input.stream.w / 2.0) * (1.0 - into);
        color = lerp(glassBehind(push.texture, pixel / float2(frame->viewport), 0.0), color, shown);
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
// surface gradient), for bumps no mesh has.
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
        // From over its surface as high as its waves can rise, down as deep as it is; no lid.
        if (face == 2)
        {
            return nothingWall();
        }
        float depth = water.size.y;
        float reach = 0.5 + frame->ocean.waveHeight * max(water.waves, 0.0);
        half = float3(water.size.x * 0.5, (depth + reach) * 0.5, water.size.z * 0.5);
        centre = float3(0.0, (reach - depth) * 0.5, 0.0);
    }
    else
    {
        // A mesh's walls would be the mesh itself, which the Obj draws if it is to be seen.
        if (water.shape == DRY_MESH)
        {
            return nothingWall();
        }
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
        return water.toWorld.c3.y + oceanMoved(frame, world.xz, poolSpacing(water), false, water.waves).y * wet;
    }
    return frame->ocean.level + oceanMoved(frame, world.xz, 0.25, false, 1.0, seaDepth(frame, world.xz)).y;
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
// waterline; how far above the waterline the near plane there is, in
// metres; how much of what the pixel shows comes through the water as far
// as it, and the water's own light over that far.
struct Under
{
    bool under;
    float line;
    float3 through;
    float3 glow;
    // Which way the pixel looks, how far it sees, and the water's surface over the near plane and its colours.
    float3 view;
    float distance;
    float surface;
    float3 absorb;
    float3 scatter;
};

// How far above the water's surface the near plane is at `pixel`, in
// metres, negative under it; `near` is that point of the near plane, and
// `surface` and `absorb` the water's height over it and how it absorbs.
// Very large where there is no water. For the sea, the height of the water
// the waves moved there, found by undoing their sideways pinch, so the
// waterline rises and falls as each wave does.
float waterlineAt(Frame* frame, float2 pixel, out float3 near, out float surface, out float3 absorb)
{
    float2 ndc = pixel / float2(frame->viewport) * 2.0 - 1.0;
    float4 nearH = transform(frame->invViewProj, float4(ndc, 1.0, 1.0));
    near = nearH.xyz / nearH.w;
    if (!waterSurface(frame, near, surface, absorb))
    {
        return 1e3;
    }
    if (frame->ocean.enabled != 0 && surface == frame->ocean.level)
    {
        float2 rest = near.xz;
        for (uint i = 0; i < 3; i++)
        {
            rest = near.xz - oceanMoved(frame, rest, 0.05, false, 1.0, seaDepth(frame, rest)).xz;
        }
        surface += oceanMoved(frame, rest, 0.05, false, 1.0, seaDepth(frame, rest)).y;
    }
    return near.y - surface;
}

Under underwaterAt(Frame* frame, float2 pixel)
{
    Under u;
    u.under = false;
    u.line = 1e3;
    u.through = 1.0;
    u.glow = 0.0;
    u.distance = 1e4;
    u.surface = -1e9;
    u.absorb = 0.0;
    u.scatter = 0.0;
    float2 ndc = pixel / float2(frame->viewport) * 2.0 - 1.0;
    float3 near;
    float surface;
    float3 absorb;
    u.line = waterlineAt(frame, pixel, near, surface, absorb);
    u.view = normalize(near - frame->cameraPosition);
    if (u.line >= 1e3)
    {
        return u;
    }
    u.surface = surface;
    u.absorb = absorb;
    if (u.line >= 0.0)
    {
        return u;
    }
    u.under = true;
    float depth = textures[NonUniformResourceIndex(push.textures.x)].Load(int3(int2(pixel), 0)).r;
    if (depth > 0.0)
    {
        float4 farH = transform(frame->invViewProj, float4(ndc, depth, 1.0));
        u.distance = length(farH.xyz / farH.w - frame->cameraPosition);
    }
    // Lost to the water as it takes each colour, and as its specks scatter all of them, so what is far off fades into
    // the water's own colour rather than showing through it, however clear.
    u.through = exp(-(absorb + UNDERWATER_SCATTERING) * u.distance);
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
    u.scatter = scatter;
    bool atmosphere = frame->lighting.sky == SKY_ATMOSPHERE;
    float3 sunLight = atmosphere ? sunThroughAtmosphere(frame) : frame->sunColor;
    Surface s;
    s.normal = float3(0.0, 1.0, 0.0);
    s.view = float3(0.0, 1.0, 0.0);
    s.roughness = 1.0;
    s.diffuse = 0.0;
    s.f0 = 0.02;
    float3 ambient = ambientLight(frame, near, s, sunLight, atmosphere);
    // Darker the deeper the camera, and darker looking down than up towards the light; brighter towards the sun,
    // as light scatters forwards.
    float3 dim = exp(-absorb * max(surface - near.y, 0.0) * 0.5);
    float up = 0.45 + 0.55 * saturate(u.view.y * 0.5 + 0.5);
    float forward = pow(saturate(dot(u.view, -frame->sunDirection)), 4.0);
    // The sun's light dimmed as far as it has come down through the water, longer the lower the sun.
    float3 sunDown = refract(frame->sunDirection, float3(0.0, 1.0, 0.0), 1.0 / 1.33);
    float3 sunDim = exp(-absorb * max(surface - near.y, 0.0) / max(-sunDown.y, 0.2));
    // The water's own colours: the sun sets how bright its light through the water is, not its hue, so a low
    // orange sun does not turn the water green.
    float sunBright = dot(sunLight, float3(0.2126, 0.7152, 0.0722));
    float3 own = deep * ambient * up + scatter * sunBright * (0.03 + 0.05 * forward) * sunDim;
    u.glow = own * dim * (1.0 - u.through);
    return u;
}

// Sunlight through the water along the pixel's view, as rays: the sun's
// shadow marched in a dozen steps as far as 30 m, each step's light focused
// into streaks by the waves over it, as caustics are, and dimmed by the
// water it has come down through and then across to the eye, scattered
// mostly onwards (Henyey–Greenstein).
float3 lightShafts(Frame* frame, Under u, float2 pixel)
{
    static const uint STEPS = 12;
    float reach = min(u.distance, 30.0);
    // A different start each frame and pixel, for the TAA to smooth.
    float jitter = frac(52.9829189 * frac(dot(pixel, float2(0.06711056, 0.00583715))) + frac(frame->time * 7.31));
    float3 down = refract(frame->sunDirection, float3(0.0, 1.0, 0.0), 1.0 / 1.33);
    float3 total = 0.0;
    for (uint i = 0; i < STEPS; i++)
    {
        float t = (float(i) + jitter) / float(STEPS) * reach;
        float3 p = frame->cameraPosition + u.view * t;
        float below = u.surface - p.y;
        if (below <= 0.0)
        {
            continue;
        }
        float2 surfaced = p.xz - down.xz / max(-down.y, 0.2) * below;
        float web = causticWeb(surfaced * 0.35, frame->time * 0.6);
        float lit = sunShadow(frame, p, float3(0.0, 1.0, 0.0));
        total += lit * (0.3 + 0.7 * web) * exp(-u.absorb * (t + below));
    }
    total *= reach / float(STEPS);
    float g = 0.6;
    float cosine = dot(u.view, -down);
    float phase = (1.0 - g * g) / (4.0 * PI * pow(max(1.0 + g * g - 2.0 * g * cosine, 1e-4), 1.5));
    bool atmosphere = frame->lighting.sky == SKY_ATMOSPHERE;
    float3 sunLight = atmosphere ? sunThroughAtmosphere(frame) : frame->sunColor;
    return total * phase * u.scatter * sunLight * 0.35;
}

// Drops on the lens just out of the water: a scatter of them, more and
// larger the wetter it is, each a little lens that bends the view through
// it and slides slowly down; how far to look aside through them, in uv,
// and how much of the pixel a drop covers.
float2 lensDrops(Frame* frame, float2 uv, float wet, out float covered)
{
    covered = 0.0;
    float2 offset = 0.0;
    float aspect = float(frame->viewport.x) / float(frame->viewport.y);
    for (uint layer = 0; layer < 2; layer++)
    {
        float across = layer == 0 ? 7.0 : 15.0;
        float2 p = uv * float2(aspect, 1.0) * across;
        float2 cell = floor(p);
        float2 r = cellPoint(cell + float(layer) * 31.7);
        // Only some cells have a drop, fewer as the lens dries.
        if (r.y > wet * 0.45)
        {
            continue;
        }
        float slid = frac(r.x * 3.7 + frame->time * (0.04 + 0.08 * r.y));
        float2 centre = float2(0.25 + 0.5 * r.x, 0.2 + 0.6 * slid);
        float radius = (0.06 + 0.22 * pow(frac(r.x * 13.1), 2.0)) * saturate(wet * 1.5);
        float2 d = frac(p) - centre;
        float inside = 1.0 - smoothstep(radius * 0.8, radius, length(d));
        // Through a drop the view is turned about, as through a small round lens.
        offset -= d / float2(aspect, 1.0) / across * 1.6 * inside;
        covered = max(covered, inside);
    }
    return offset;
}

// The view under water and across the waterline, over a copy of the
// picture (push.texture, with mips): under the surface, what the view shows
// dimmed by the water it comes through colour by colour, red first, the
// water's own light over it and the sun's rays through it; at the
// waterline, a band a few pixels wide that bends and blurs what is behind
// it as the water's edge on the lens does, a dark line along the meniscus
// and a bright rim over it; and just out of the water, drops on the lens
// (push.textures.y, how wet it is). Pixels with none of these keep what
// they had. push.textures.x is the depth.
[shader("fragment")]
float4 underwaterFragment(UnderOut input) : SV_Target0
{
    Frame* frame = push.frame;
    float2 pixel = input.position.xy;
    float2 uv = pixel / float2(frame->viewport);
    Under u = underwaterAt(frame, pixel);
    // How many pixels above the waterline this one is, negative under it.
    float above = u.line / max(fwidth(u.line), 1e-7);
    float band = 1.0 - saturate(abs(above) / 10.0);
    float wet = textures[NonUniformResourceIndex(push.textures.y)].Load(int3(0, 0, 0)).r;
    float lens = cameraUnderwater(frame) || !oceanHas(frame, OCEAN_WET_LENS) ? 0.0 : wet;
    if (!u.under && band <= 0.0 && lens <= 0.01)
    {
        discard;
    }
    float covered;
    float2 offset = lens > 0.01 ? lensDrops(frame, uv, lens, covered) : 0.0;
    float2 texel = 1.0 / float2(frame->viewport);
    // The water's edge on the lens: what is behind pulled towards the line, swaying along it.
    float ripple = sin(pixel.x * 0.045 + frame->time * 2.3) * 3.0 + sin(pixel.x * 0.13 - frame->time * 3.7) * 1.5;
    offset += float2(sin(pixel.x * 0.06 + frame->time * 3.0) * 1.5, -sign(above) * 5.0 + ripple) * band * texel;
    float blur = band * 1.5 + pow(lens, 6.0) * 3.0 + (u.under ? 0.5 : 0.0);
    float3 color = glassBehind(push.texture, uv + offset, blur);
    if (u.under)
    {
        color = color * u.through + u.glow + (oceanHas(frame, OCEAN_LIGHT_SHAFTS) ? lightShafts(frame, u, pixel) : 0.0);
    }
    // The meniscus: a dark line where the water meets the lens, and the bright rim of its curve over it.
    float line = 1.0 - smoothstep(0.5, 2.0, abs(above));
    float rim = smoothstep(1.0, 2.5, above) * (1.0 - smoothstep(2.5, 6.0, above));
    color = color * (1.0 - 0.6 * line) + rim * (color * 0.3 + 0.02);
    return float4(color, 1.0);
}

// How wet the camera's lens is: 1 under water, then drying over a couple of
// seconds once out, from last frame's (push.textures.x; push.textures.y is
// 1 on the first frame, when there is none), into the 1×1 storage image
// push.output.
[shader("compute")]
[numthreads(1, 1, 1)]
void lensWetnessMain(uint3 id : SV_DispatchThreadID)
{
    Frame* frame = push.frame;
    float before = push.textures.y != 0 ? 0.0 : textures[NonUniformResourceIndex(push.textures.x)].Load(int3(0, 0, 0)).r;
    float now = cameraUnderwater(frame) ? 1.0 : before * exp(-frame->deltaTime / 1.2);
    storageImagesR32f[push.output][uint2(0, 0)] = saturate(now);
}
