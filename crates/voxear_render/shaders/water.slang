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
    // SEA, or the Pool it is of.
    nointerpolation uint body : BODY;
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
    float3 world = flat + oceanMoved(frame, flat.xz, spacing, false, body.waves);
    float3 before = flat + oceanMoved(frame, flat.xz, spacing, true, body.waves);
    WaterOut o;
    o.position = transform(frame->viewProj, float4(world, 1.0));
    o.world = world;
    o.rest = flat.xz;
    o.currentClip = transform(frame->unjitteredViewProj, float4(world, 1.0));
    o.previousClip = transform(frame->prevViewProj, float4(before, 1.0));
    o.body = pool;
    return o;
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

// How far `world` is outside Dry Area `body`, negative inside.
float dryDistance(WaterBody body, float3 world)
{
    float3 local = transform(body.toLocal, float4(world, 1.0)).xyz;
    float3 half = body.size * 0.5;
    if (body.shape == DRY_SPHERE)
    {
        return length(local) - half.x;
    }
    if (body.shape == DRY_CAPSULE)
    {
        float reach = max(half.y - half.x, 0.0);
        return length(local - float3(0.0, clamp(local.y, -reach, reach), 0.0)) - half.x;
    }
    float3 q = abs(local) - half;
    return length(max(q, 0.0)) + min(max(q.x, max(q.y, q.z)), 0.0);
}

// Whether water drawn at `world` is kept out: inside a Dry Area; and, for
// the sea, over a Pool lower than it, which is dry to the sea.
bool keptDry(Frame* frame, float3 world, uint body)
{
    uint pools = frame->ocean.pools;
    for (uint i = pools; i < pools + frame->ocean.dries; i++)
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
    for (uint c = 0; c < OCEAN_CASCADES; c++)
    {
        float2 uv = cascadeUv(frame, c, rest);
        float4 s = textures[NonUniformResourceIndex(frame->ocean.slopes[c])].Sample(samplers[SAMPLER_LINEAR_REPEAT], uv);
        float4 d = textures[NonUniformResourceIndex(frame->ocean.displacement[c])].Sample(samplers[SAMPLER_LINEAR_REPEAT], uv);
        float share = cascadeShare(c, waves);
        slope += s.xy * share;
        // The slopes the mips averaged away, too small to see here, as roughness (Atlas).
        variance += (max(s.z - s.x * s.x, 0.0) + max(s.w - s.y * s.y, 0.0)) * share * share;
        folded += (d.w - 1.0) * share;
        moved += d.xyz * share;
    }
    float3 n = normalize(float3(-slope.x, 1.0, -slope.y));
    float3 v = normalize(frame->cameraPosition - input.world);
    // Seen from below, the surface faces down.
    n = dot(n, v) < 0.0 && v.y < 0.0 ? -n : n;
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
    float crest = saturate(moved.y / (height * 0.5)) + saturate(length(moved.xz) / height) * 0.5;
    float towards = pow(saturate(dot(l, -v)), 4.0) * pow(saturate(0.5 - 0.5 * dot(l, n)), 3.0);
    float3 throughWave = (1.5 * crest * towards + 0.12 * pow(saturate(dot(v, n)), 2.0) + 0.04 * saturate(dot(l, n))) * scatter * sunLight * shadow;
    float3 body = deep * ambient + throughWave;

    // What lies under it: how far through the water, bent by the waves, and dimmed a colour at a time.
    float2 pixel = input.position.xy;
    float behindDepth = textures[NonUniformResourceIndex(push.textures.y)].Load(int3(int2(pixel), 0)).r;
    float surfaceDepth = frame->near / max(input.position.z, 1e-7);
    float sceneDepth = behindDepth > 0.0 ? frame->near / behindDepth : 1e4;
    float along = max(dot(-v, frame->cameraForward), 0.05);
    float thickness = max(sceneDepth - surfaceDepth, 0.0) / along;
    // A Pool's water ends at its sides and bottom, whatever lies past them.
    thickness = sea ? thickness : min(thickness, poolExit(pool, input.world, v));
    float3 through = exp(-absorb * thickness);
    float2 bend = n.xz * 0.04 * saturate(thickness);
    float blur = sqrt(s.roughness) * 2.0 + log2(1.0 + thickness * 0.5);
    float3 behind = thickness < 1e3 ? glassBehind(push.texture, pixel / float2(frame->viewport) - bend, blur) : 0.0;
    float3 under = behind * through + body * (1.0 - through);
    float3 color = (1.0 - fresnel) * under + fresnel * reflected + glints;

    // Foam where a crest folds over, and where the water thins at a shore, in bubbles drifting with the waves.
    // Only where the water folds hard does a crest break: a calm sea's gentle folding stays clear.
    float threshold = 0.4 + 0.45 * frame->ocean.foam;
    float breaking = frame->ocean.foam > 0.0 ? saturate((threshold - folded) / 0.3) : 0.0;
    float shore = saturate(1.0 - thickness / 0.6);
    float lace = waterCells(rest * 1.3 + moved.xz * 0.5) * 0.7 + waterCells(rest * 4.1) * 0.3;
    float foam = saturate((max(breaking, shore) * 1.4 - lace) * 4.0);
    float3 foamLit = 0.85 * (ambient + sunLight * shadow * saturate(dot(n, l)) / PI);
    color = lerp(color, foamLit, foam);

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
        return water.toWorld.c3.y + oceanMoved(frame, world.xz, 0.25, false, water.waves).y;
    }
    return frame->ocean.level + oceanMoved(frame, world.xz, 0.25, false).y;
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
