// A lightmap bake's path tracer (src/lightmapper.rs), against the scene the
// GI's rays see, by the GPU's ray queries or in software: included at the
// end of ddgi.slang, so it is built both ways.
//
// From each lightmap texel, paths go out over the hemisphere above it,
// spread by the cosine, so their light averaged is the light a surface
// there takes, as the GI's probes give it to shading. Each path bounces up
// to the bake's bounces: at each surface it meets, it takes that surface's
// glow and the direct light on it of the lights the bake keeps (Mixed and
// Baked, and the sun unless it is Realtime), each through a shadow ray to a
// point on the light's shape, so baked shadows are as soft as the lights
// are wide; then on, by the surface's colour. A path that leaves the scene
// takes the sky's light. The Baked lights' direct light on the texel itself
// is added too. Each texel also keeps which way its light mostly came, for
// directional lightmaps, and how many of its paths first met the back of
// something, which says it lies inside a wall.
//
// Light probes trace twice as many paths each, over the whole sphere, into
// nine spherical harmonics a colour.
//
// Every path's randomness comes from its texel or probe and its number, so a
// bake of the same scene comes out the same.

// The brightest one path may bring back, by its brightest channel: a path that
// meets a small bright glow is rare, and unclamped it leaves a speck.
static const float BAKE_PATH_MOST = 64.0;
// How far off a surface a bake's ray starts: a hair, and a share of the texel there.
static const float BAKE_OFFSET = 0.003;
static const float BAKE_OFFSET_TEXEL = 0.05;

uint bakeHash(uint a, uint b)
{
    return ddgiHash(a * 1664525u + ddgiHash(b ^ 0x9E3779B9u));
}

// The next of a path's random numbers, from 0 to 1.
float bakeRandom(inout uint state)
{
    state = state * 747796405u + 2891336453u;
    uint word = ((state >> ((state >> 28u) + 4u)) ^ state) * 277803737u;
    word = (word >> 22u) ^ word;
    return float(word >> 8) / 16777216.0;
}

// A path's first two random numbers: an even sequence (Roberts' R2) moved by
// its texel's or probe's own, so neighbours do not share their pattern.
float2 bakeFirst(uint sample, uint owner)
{
    float2 shift = float2(ddgiHash(owner), ddgiHash(owner ^ 0x5bd1e995u)) / 4294967296.0;
    return frac(float2(float(sample) * 0.7548776662, float(sample) * 0.5698402910) + shift);
}

float bakeLuminance(float3 c)
{
    return dot(c, float3(0.2126, 0.7152, 0.0722));
}

// Two directions at right angles to `n` and each other.
void bakeBasis(float3 n, out float3 t, out float3 b)
{
    t = normalize(cross(abs(n.y) < 0.999 ? float3(0.0, 1.0, 0.0) : float3(1.0, 0.0, 0.0), n));
    b = cross(n, t);
}

// A direction over the hemisphere round `n`, as many towards it as its cosine says.
float3 cosineDirection(float3 n, float2 u)
{
    float3 t, b;
    bakeBasis(n, t, b);
    float r = sqrt(u.x);
    float angle = 2.0 * PI * u.y;
    return normalize(t * (r * cos(angle)) + b * (r * sin(angle)) + n * sqrt(max(1.0 - u.x, 0.0)));
}

// A direction over the whole sphere, evenly.
float3 sphereDirection(float2 u)
{
    float y = 1.0 - 2.0 * u.x;
    float ring = sqrt(max(1.0 - y * y, 0.0));
    float angle = 2.0 * PI * u.y;
    return float3(cos(angle) * ring, y, sin(angle) * ring);
}

// A point on a lamp's shape, for a shadow ray to aim at.
float3 lampPoint(Light lamp, float2 u)
{
    float3 up = cross(lamp.direction, lamp.right);
    switch (lamp.kind)
    {
    case LIGHT_RECTANGLE:
        return lamp.position + lamp.right * ((u.x * 2.0 - 1.0) * lamp.size.x) + up * ((u.y * 2.0 - 1.0) * lamp.size.y);
    case LIGHT_DISC:
    {
        float r = lamp.size.x * sqrt(u.x);
        float angle = 2.0 * PI * u.y;
        return lamp.position + lamp.right * (r * cos(angle)) + up * (r * sin(angle));
    }
    case LIGHT_TUBE:
        return lamp.position + lamp.right * ((u.x * 2.0 - 1.0) * lamp.size.x);
    default:
        return lamp.position + sphereDirection(u) * (lamp.shadowRadius * sqrt(u.x));
    }
}

// A lamp's light on a white surface, before its shadow.
float3 unshadowedLamp(Surface s, Light lamp, float3 position)
{
    switch (lamp.kind)
    {
    case LIGHT_RECTANGLE:
        return shadeRectangleLight(s, lamp, position);
    case LIGHT_DISC:
        return shadeDiscLight(s, lamp, position);
    case LIGHT_TUBE:
        return shadeTubeLight(s, lamp, position);
    default:
        return shadePunctualLight(s, lamp, position);
    }
}

// The light a surface facing `normal` at `position` takes straight from the
// bake's lights before `lights`, and the sun if `sun`, for each unit of its
// colour; `way` gains the way it came, weighed by its brightness.
float3 bakeDirect(Frame* frame, float3 position, float3 normal, uint lights, bool sun, inout uint rng, inout float4 way)
{
    float3 light = 0.0;
    float3 origin = position + normal * BAKE_OFFSET;
    if (sun)
    {
        float3 toSun = -frame->sunDirection;
        // Somewhere on the sun's disc, so baked sun shadows soften as the real-time ones do.
        float3 t, b;
        bakeBasis(toSun, t, b);
        float r = frame->lighting.sunSoftness * sqrt(bakeRandom(rng));
        float angle = 2.0 * PI * bakeRandom(rng);
        toSun = normalize(toSun + t * (r * cos(angle)) + b * (r * sin(angle)));
        float facing = dot(normal, toSun);
        if (facing > 0.0 && !ddgiBlocked(frame, origin, toSun, 1e4))
        {
            bool atmosphere = frame->lighting.sky == SKY_ATMOSPHERE;
            float3 sunLight = atmosphere ? sunThroughAtmosphere(frame) : frame->sunColor;
            float3 lit = sunLight * facing / PI;
            light += lit;
            way += float4(toSun, 1.0) * bakeLuminance(lit);
        }
    }
    Surface s;
    s.normal = normal;
    s.view = normal;
    s.diffuse = 1.0;
    s.f0 = 0.0;
    s.roughness = 1.0;
    for (uint i = 0; i < min(lights, frame->lightCount); i++)
    {
        Light lamp = buffers()->lights[i];
        if (distance(lamp.position, position) >= lamp.range + 2.0 * max(lamp.size.x, lamp.size.y))
        {
            continue;
        }
        float3 lit = unshadowedLamp(s, lamp, position);
        if (all(lit <= 0.0))
        {
            continue;
        }
        float3 target = lampPoint(lamp, float2(bakeRandom(rng), bakeRandom(rng)));
        float3 toLamp = target - origin;
        float far = length(toLamp);
        if (far > 0.02 && ddgiBlocked(frame, origin, toLamp / far, far - 0.02))
        {
            continue;
        }
        light += lit;
        way += float4(normalize(lamp.position - position), 1.0) * bakeLuminance(lit);
    }
    return light;
}

// The light a path brings back from `origin` along `direction`, bouncing up
// to `bounces` times; `back` when its first surface is met from behind.
float3 bakePath(Frame* frame, float3 origin, float3 direction, uint bounces, inout uint rng, out bool back)
{
    back = false;
    float3 throughput = 1.0;
    float3 light = 0.0;
    bool sun = frame->bake.sunMode != LIGHT_MODE_REALTIME;
    for (uint bounce = 0; bounce < max(bounces, 1u); bounce++)
    {
        Hit hit = traceScene(frame, origin, direction, 1e4, false);
        if (!hit.found)
        {
            light += throughput * ddgiSky(frame, direction);
            break;
        }
        // Textures blurred well down, as the GI's rays read them: a texel's light is an average over many paths.
        HitSurface surface = hitSurface(frame, hit.instance, hit.primitive, hit.barycentrics, direction, 2.0);
        if (surface.back)
        {
            back = bounce == 0;
            break;
        }
        float3 position = origin + direction * hit.t;
        float4 unused = 0.0;
        float3 direct = bakeDirect(frame, position, surface.normal, frame->bake.bakeLights, sun, rng, unused);
        light += throughput * (surface.emissive + surface.albedo * direct);
        throughput *= surface.albedo;
        if (max(max(throughput.r, throughput.g), throughput.b) < 1e-3)
        {
            break;
        }
        origin = position + surface.normal * BAKE_OFFSET;
        direction = cosineDirection(surface.normal, float2(bakeRandom(rng), bakeRandom(rng)));
    }
    float most = max(max(light.r, light.g), light.b);
    return most > BAKE_PATH_MOST ? light * (BAKE_PATH_MOST / most) : light;
}

// A thread a texel, tracing its paths for this dispatch. push.level: which
// of this frame's jobs it is; push.size.x: how many texels it traces, from
// the job's first.
[shader("compute")]
[numthreads(64, 1, 1)]
void lightBakeTexelsMain(uint3 id : SV_DispatchThreadID)
{
    bakeTracing = true;
    Frame* frame = push.frame;
    BakeJob* job = buffers()->bakeJob + push.level;
    uint index = job->firstTexel + id.x;
    if (id.x >= push.size.x || index >= job->texelCount)
    {
        return;
    }
    BakeTexel texel = job->texels[index];
    float3 position = texel.position.xyz;
    float3 normal = texel.normal.xyz;
    float3 face = texel.face.xyz;
    float3 origin = position + face * (BAKE_OFFSET + texel.position.w * BAKE_OFFSET_TEXEL);
    bool sunBaked = frame->bake.sunMode == LIGHT_MODE_BAKED;
    float3 sum = 0.0;
    float4 way = 0.0;
    float backs = 0.0;
    for (uint k = 0; k < job->samples; k++)
    {
        uint sample = job->firstSample + k;
        uint rng = bakeHash(index, sample);
        float3 direction = cosineDirection(normal, bakeFirst(sample, index));
        // Kept above the triangle itself, where its smoothed normal leans past it.
        direction = dot(direction, face) <= 0.0 ? reflect(direction, face) : direction;
        bool back;
        float3 light = bakePath(frame, origin, direction, job->bounces, rng, back);
        if (back)
        {
            backs += 1.0;
            continue;
        }
        float3 direct = bakeDirect(frame, position, normal, frame->bake.bakedLights, sunBaked, rng, way);
        way += float4(direction, 1.0) * bakeLuminance(light);
        sum += light + direct;
    }
    job->light[index] += float4(sum, backs);
    job->directions[index] += way;
}

groupshared float4 bakeShared[64][9];

// A workgroup a probe, its threads sharing its paths for this dispatch,
// twice as many as a texel's, over the whole sphere.
[shader("compute")]
[numthreads(64, 1, 1)]
void lightBakeProbesMain(uint3 group : SV_GroupID, uint3 thread : SV_GroupThreadID)
{
    bakeTracing = true;
    Frame* frame = push.frame;
    BakeJob* job = buffers()->bakeJob + push.level;
    uint probe = group.x;
    if (probe >= job->probeCount)
    {
        return;
    }
    float3 position = job->probes[probe].xyz;
    float3 light[9];
    for (uint i = 0; i < 9; i++)
    {
        light[i] = 0.0;
    }
    float backs = 0.0;
    uint owner = probe | 0x80000000u;
    for (uint k = thread.x; k < job->samples * 2; k += 64)
    {
        uint sample = job->firstSample * 2 + k;
        uint rng = bakeHash(owner, sample);
        float3 direction = sphereDirection(bakeFirst(sample, owner));
        bool back;
        float3 found = bakePath(frame, position, direction, job->bounces, rng, back);
        if (back)
        {
            backs += 1.0;
            continue;
        }
        float y[9];
        bakeHarmonics(direction, y);
        for (uint i = 0; i < 9; i++)
        {
            light[i] += found * y[i];
        }
    }
    for (uint i = 0; i < 9; i++)
    {
        bakeShared[thread.x][i] = float4(light[i], i == 0 ? backs : 0.0);
    }
    GroupMemoryBarrierWithGroupSync();
    // Added up in the same order every time, so the bake is the same every time.
    if (thread.x < 9)
    {
        float4 total = 0.0;
        for (uint t = 0; t < 64; t++)
        {
            total += bakeShared[t][thread.x];
        }
        job->probeLight[probe * 9 + thread.x] += total;
    }
}
