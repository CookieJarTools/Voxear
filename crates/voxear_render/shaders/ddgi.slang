// Ray-traced GI's probes (ddgi_sample.slang reads them), filled each frame
// in two compute passes, for the probes due:
// - rays: each probe traces 256 rays against the scene's triangles, turned a
//   little each frame. Where a ray hits, it takes the light that surface
//   sends back: the sun's, where a second ray finds nothing between, the
//   lamps', its glow, and the light the probes held last frame round it, so
//   light bounces on. Where it misses, the sky's. A ray that hits the back
//   of a surface is marked so: the probe is inside something.
// - update: each texel of a probe's maps blends in the rays round its
//   direction, the light they found and how far they went, and each probe
//   is kept or set aside: one that sees mostly backs of surfaces is inside a
//   wall, and lights nothing.
//
// Built twice: with the GPU's ray tracing, and with SOFTWARE_RAYS, walking
// hierarchies of the scene's triangles in the compute shader
// (trace_software.slang), for GPUs without it.

#include "shading.slang"
#include "ddgi_sample.slang"
#include "ambient_occlusion.slang"

#ifndef SOFTWARE_RAYS
#define SOFTWARE_RAYS 0
#endif

// What a ray found: whether it hit, how far, the instance and triangle, and
// where on the triangle.
struct Hit
{
    bool found;
    float t;
    uint instance;
    uint primitive;
    float2 barycentrics;
};

#if SOFTWARE_RAYS
#include "trace_software.slang"
#else
[[vk::binding(4, 0)]] RaytracingAccelerationStructure scenes[];

// The nearest triangle along a ray, or any at all with `any`, by the GPU's
// ray tracing, through the scene's top level.
Hit traceScene(Frame* frame, float3 origin, float3 direction, float far, bool any)
{
    RaytracingAccelerationStructure scene = scenes[NonUniformResourceIndex(frame->ddgi.scene)];
    RayDesc ray;
    ray.Origin = origin;
    ray.Direction = direction;
    ray.TMin = 0.0;
    ray.TMax = far;
    Hit hit;
    hit.found = false;
    if (any)
    {
        RayQuery<RAY_FLAG_ACCEPT_FIRST_HIT_AND_END_SEARCH | RAY_FLAG_FORCE_OPAQUE> query;
        query.TraceRayInline(scene, RAY_FLAG_NONE, 0xFF, ray);
        query.Proceed();
        hit.found = query.CommittedStatus() == COMMITTED_TRIANGLE_HIT;
        return hit;
    }
    RayQuery<RAY_FLAG_FORCE_OPAQUE> query;
    query.TraceRayInline(scene, RAY_FLAG_NONE, 0xFF, ray);
    query.Proceed();
    hit.found = query.CommittedStatus() == COMMITTED_TRIANGLE_HIT;
    if (hit.found)
    {
        hit.t = query.CommittedRayT();
        hit.instance = query.CommittedInstanceID();
        hit.primitive = query.CommittedPrimitiveIndex();
        hit.barycentrics = query.CommittedTriangleBarycentrics();
    }
    return hit;
}
#endif

// How much of a probe's last value a fresh one keeps, against noise. At 0.95, the noise left in each update made
// walls lit by candles and a neon tube shimmer; 0.97 and twice the rays halve that, and light that falls is let
// go of at once all the same (DDGI_FALL).
static const float DDGI_KEEP = 0.97;
// A fall in a probe's light, as a share of it a frame and as its last few frames have kept it up, from which it
// keeps less of its last value, to where it keeps none: under the first, the rays' own noise; over the second,
// the light really fell.
static const float2 DDGI_FALL = float2(0.12, 0.3);
// How much of a frame's change goes into the running change a probe texel keeps. One frame's change is mostly
// the rays' noise, which with small bright lamps about (candles) swings well past the thresholds and flickers;
// noise up one frame is down the next, so it averages out, where light that really fell keeps falling.
static const float DDGI_TREND = 0.3;
// The brightest a probe's ray may find, by its brightest channel. A lamp inside or against a mesh (a neon tube in
// its glass) lights that mesh's faces from a few centimetres, thousands of times as brightly as from a metre;
// whether a probe's rays land on such a sliver changes as they turn and as the probe moves, and its light then
// swung the bounce of a whole corridor to twice as bright for frames at a time. Lit and glowing surfaces are far
// below this.
static const float DDGI_RAY_MOST = 16.0;
// Rays that find the back of a surface in this share of a probe's rays put it inside something.
static const float DDGI_INSIDE = 0.25;

// Directions spread evenly over the sphere, on a Fibonacci spiral, turned
// by this frame's rotation.
float3 ddgiRayDirection(Frame* frame, uint ray)
{
    float y = 1.0 - (float(ray) + 0.5) * 2.0 / float(DDGI_RAYS);
    float ring = sqrt(saturate(1.0 - y * y));
    float angle = float(ray) * 2.39996323;
    float3 d = float3(cos(angle) * ring, y, sin(angle) * ring);
    float4 r0 = frame->ddgi.rotation[0];
    float4 r1 = frame->ddgi.rotation[1];
    float4 r2 = frame->ddgi.rotation[2];
    return normalize(float3(dot(r0.xyz, d), dot(r1.xyz, d), dot(r2.xyz, d)));
}

uint ddgiHash(uint index)
{
    uint h = index * 747796405u + 2891336453u;
    h = ((h >> ((h >> 28u) + 4u)) ^ h) * 277803737u;
    return (h >> 22u) ^ h;
}

// A probe's state, in its w: inside something, so it is never used; asleep,
// with nothing within a spacing of it, so it lights only the air round it
// (the mist) and is traced far less often; or awake, lighting surfaces.
static const float DDGI_ASLEEP = 0.75;
// How many times less often a sleeping probe traces than an awake one.
static const uint DDGI_SLEEP = 8;

// Whether a probe traces this frame, and whether it is new to the grid, as
// the grid has just moved over it, with nothing kept from before. `state` is
// its state's w.
bool ddgiDue(Frame* frame, uint cascade, uint index, float state, out int3 world, out bool fresh)
{
    int3 origin = frame->ddgi.origin[cascade].xyz;
    world = ddgiWorldOf(ddgiSlotOf(index), origin);
    int3 before = frame->ddgi.previousOrigin[cascade].xyz;
    fresh = frame->ddgi.warmUp > 0 || any(world < before) || any(world >= before + DDGI_PROBES);
    // A quarter of the nearest cascade's probes each frame, which light what is near and must answer a torch
    // carried by, an eighth of the next and a sixteenth of the farthest, which light only the distance; scattered
    // through the grid; half as many where the rays are traced in software, and those asleep far less often still.
    uint every = (4u << min(cascade, 2u)) * (SOFTWARE_RAYS ? 2u : 1u);
    every *= (state > 0.5 && state < 0.9) ? DDGI_SLEEP : 1u;
    return fresh || (ddgiHash(index + cascade * 65536u) + frame->frameIndex) % every == 0;
}

int2 ddgiRayTexel(uint index, uint cascade, uint ray)
{
    return int2(ray + (index % 16u) * DDGI_RAYS, index / 16u + cascade * (DDGI_PROBE_COUNT / 16u));
}

bool ddgiBlocked(Frame* frame, float3 origin, float3 direction, float far)
{
    return traceScene(frame, origin, direction, far, true).found;
}

// The light a surface sends back along a ray that hit it, where it hit, and
// whether the ray found the surface's back, as from inside something.
float3 ddgiShadeHit(Frame* frame, uint id, uint primitive, float2 barycentrics,
    float3 position, float3 direction, float mip, out bool back)
{
    Instance instance = buffers()->instances[id];
    Mesh mesh = buffers()->meshes[instance.mesh];
    uint first = mesh.lods[0].indexOffset + primitive * 3;
    Vertex a = buffers()->vertices[mesh.vertexOffset + buffers()->indices[first]];
    Vertex b = buffers()->vertices[mesh.vertexOffset + buffers()->indices[first + 1]];
    Vertex c = buffers()->vertices[mesh.vertexOffset + buffers()->indices[first + 2]];
    float3 weights = float3(1.0 - barycentrics.x - barycentrics.y, barycentrics.x, barycentrics.y);
    float3 local = a.normal * weights.x + b.normal * weights.y + c.normal * weights.z;
    float3 normal = normalize(instance.normal0.xyz * local.x + instance.normal1.xyz * local.y + instance.normal2.xyz * local.z);
    // Its normals, not its winding, say which side is its front.
    back = dot(normal, direction) > 0.0;
    // Both faces of a surface take light, as its front does.
    normal = back ? -normal : normal;
    float2 uv = a.uv * weights.x + b.uv * weights.y + c.uv * weights.z;
    Material material = buffers()->materials[instance.material];
    float4 base = material.baseColor
        * textures[NonUniformResourceIndex(material.baseColorTexture)].SampleLevel(samplers[SAMPLER_LINEAR_REPEAT], uv, mip);
    // Metalness as shading has it: the material's factor times its texture's blue, which glTF factors leave at 1.
    float metal = material.metallic
        * textures[NonUniformResourceIndex(material.metallicRoughnessTexture)].SampleLevel(samplers[SAMPLER_LINEAR_REPEAT], uv, mip).b;
    // As the season has it, so bounced light and reflections turn with the year too.
    float rough = material.roughness;
    seasonalSurface(frame, material, instance.model.c3.xyz, normal, base.rgb, rough);
    float3 albedo = base.rgb * (1.0 - metal);
    float3 emissive = material.emissive
        * textures[NonUniformResourceIndex(material.emissiveTexture)].SampleLevel(samplers[SAMPLER_LINEAR_REPEAT], uv, mip).rgb
        * glowAmount(frame, material, instance.model.c3.xyz);

    float3 toSun = -frame->sunDirection;
    bool atmosphere = frame->lighting.sky == SKY_ATMOSPHERE;
    float3 sunLight = atmosphere ? sunThroughAtmosphere(frame) : frame->sunColor;
    float facing = dot(normal, toSun);
    float3 lit = 0.0;
    if (facing > 0.0 && !ddgiBlocked(frame, position + normal * 0.02, toSun, 1e4))
    {
        lit += albedo / PI * sunLight * facing;
    }
    Surface s;
    s.normal = normal;
    s.view = -direction;
    s.diffuse = albedo;
    s.f0 = 0.0;
    s.roughness = 1.0;
    for (uint i = 0; i < frame->lightCount; i++)
    {
        Light lamp = buffers()->lights[i];
        if (distance(lamp.position, position) < lamp.range + 2.0 * max(lamp.size.x, lamp.size.y))
        {
            lit += shadeLocalLight(s, lamp, position);
        }
    }
    // Last frame's light round it, so light bounces again.
    float up = normal.y * 0.5 + 0.5;
    float3 sky = atmosphere ? atmosphereAmbient(frame, up, sunLight)
        : lerp(frame->groundColor, lerp(frame->skyHorizon, frame->skyZenith, 0.5), up) * 0.35 * frame->ambient;
    float3 around = ddgiIrradiance(frame, position, normal, -direction, sky);
    return lit + albedo * around + emissive;
}

// The sky's light from a direction, as the plain sky light has it.
float3 ddgiSky(Frame* frame, float3 direction)
{
    float up = direction.y * 0.5 + 0.5;
    if (frame->lighting.sky == SKY_ATMOSPHERE)
    {
        return atmosphereAmbient(frame, up, sunThroughAtmosphere(frame));
    }
    float3 above = lerp(frame->skyHorizon, frame->skyZenith, 0.5);
    return lerp(frame->groundColor, above, up) * 0.35 * frame->ambient;
}

// One workgroup a probe, a thread a ray. push.output: the rays.
[shader("compute")]
[numthreads(256, 1, 1)]
void ddgiRaysMain(uint3 group : SV_GroupID, uint3 thread : SV_GroupThreadID)
{
    Frame* frame = push.frame;
    uint cascade = group.x / DDGI_PROBE_COUNT;
    uint index = group.x % DDGI_PROBE_COUNT;
    int3 world;
    bool fresh;
    float state = textures[NonUniformResourceIndex(frame->ddgi.probes)].Load(int3(ddgiTile(index, cascade), 0)).w;
    if (cascade >= DDGI_CASCADES || !ddgiDue(frame, cascade, index, state, world, fresh))
    {
        return;
    }
    float3 origin = ddgiProbePosition(frame, cascade, world);
    float3 direction = ddgiRayDirection(frame, thread.x);
    Hit hit = traceScene(frame, origin, direction, 1e4, false);
    float4 result;
    if (!hit.found)
    {
        result = float4(ddgiSky(frame, direction), 1e4);
    }
    else
    {
        float t = hit.t;
        float3 position = origin + direction * t;
        bool back;
        // Textures blurred well down: a probe's light is an average over many rays anyway.
        float3 light = ddgiShadeHit(frame, hit.instance, hit.primitive, hit.barycentrics, position, direction, 3.0, back);
        light *= min(1.0, DDGI_RAY_MOST / max(max(max(light.r, light.g), light.b), 1e-6));
        // The back of a surface, seen from inside it: no light, and marked by its distance's sign.
        result = back ? float4(0.0, 0.0, 0.0, -t) : float4(light, t);
    }
    storageImagesRgba16f[push.output][ddgiRayTexel(index, cascade, thread.x)] = result;
}

// The probe's rays and their directions, read once by the whole workgroup
// for the one thread that moves the probe, which reads every ray. The
// texels' threads read the rays themselves: from shared memory the loop
// over them ran five times slower on an RTX 3070.
groupshared float4 sharedRays[DDGI_RAYS];
groupshared float3 sharedDirections[DDGI_RAYS];

// A probe's new state: where it has moved to, in spacings from its cell's
// middle, and whether it is inside something, asleep or awake. A probe too near a surface moves
// away from it, so it does not sit on a roof or in a wall's face and see
// both sides; one inside something moves out through the nearest back it
// sees; and one still inside after that is set aside (DDGI's relocation).
float4 ddgiRelocate(Frame* frame, uint cascade, int2 tile, bool fresh)
{
    // Nearer than this to a surface, in spacings, a probe moves away; it moves at most this far from its middle.
    static const float NEAREST = 0.3;
    static const float FARTHEST = 0.45;
    float spacing = ddgiSpacing(frame, cascade);
    float3 moved = fresh ? 0.0 : storageImagesRgba16f[push.textures.w][tile].xyz;
    float nearestFront = 1e9;
    float3 towardsFront = 0.0;
    float nearestBack = 1e9;
    float3 towardsBack = 0.0;
    uint backs = 0;
    for (uint ray = 0; ray < DDGI_RAYS; ray++)
    {
        float reach = sharedRays[ray].a / spacing;
        float3 direction = sharedDirections[ray];
        if (reach < 0.0)
        {
            backs++;
            if (-reach < nearestBack)
            {
                nearestBack = -reach;
                towardsBack = direction;
            }
        }
        else if (reach < nearestFront)
        {
            nearestFront = reach;
            towardsFront = direction;
        }
    }
    bool inside = float(backs) > DDGI_INSIDE * float(DDGI_RAYS);
    if (inside)
    {
        // Out through the back it is nearest, and a little past it.
        moved += towardsBack * (nearestBack + NEAREST * 0.5);
    }
    else if (nearestFront < NEAREST)
    {
        moved -= towardsFront * (NEAREST - nearestFront);
    }
    moved = clamp(moved, -FARTHEST, FARTHEST);
    // Nothing within a spacing: it lights no surface, only the air, and sleeps.
    float state = inside ? 0.0 : (nearestFront > 1.0 && backs == 0 ? DDGI_ASLEEP : 1.0);
    return float4(moved, state);
}

// One workgroup a probe, a thread a texel of its maps. push.textures: x the
// rays, y the irradiance and z the depth atlases and w the probes' states,
// as storage images.
[shader("compute")]
[numthreads(8, 8, 1)]
void ddgiUpdateMain(uint3 group : SV_GroupID, uint3 thread : SV_GroupThreadID, uint flat : SV_GroupIndex)
{
    Frame* frame = push.frame;
    uint cascade = group.x / DDGI_PROBE_COUNT;
    uint index = group.x % DDGI_PROBE_COUNT;
    int3 world;
    bool fresh;
    if (cascade >= DDGI_CASCADES)
    {
        return;
    }
    int2 tile = ddgiTile(index, cascade);
    float state = storageImagesRgba16f[push.textures.w][tile].w;
    if (!ddgiDue(frame, cascade, index, state, world, fresh))
    {
        return;
    }
    // The workgroup's 64 threads read the probe's 256 rays, four each.
    for (uint ray = flat; ray < DDGI_RAYS; ray += 64)
    {
        sharedRays[ray] = storageImagesRgba16f[push.textures.x][ddgiRayTexel(index, cascade, ray)];
        sharedDirections[ray] = ddgiRayDirection(frame, ray);
    }
    GroupMemoryBarrierWithGroupSync();
    float3 direction = octDecode((float2(thread.xy) + 0.5) / float(DDGI_TEXELS) * 2.0 - 1.0);
    float far = ddgiSpacing(frame, cascade) * 1.5;
    float3 light = 0.0;
    float lightWeights = 0.0;
    float2 depth = 0.0;
    float depthWeights = 0.0;
    uint backs = 0;
    for (uint ray = 0; ray < DDGI_RAYS; ray++)
    {
        float4 found = storageImagesRgba16f[push.textures.x][ddgiRayTexel(index, cascade, ray)];
        float3 rayDirection = ddgiRayDirection(frame, ray);
        float facing = max(dot(rayDirection, direction), 0.0);
        light += found.rgb * facing;
        lightWeights += facing;
        float reach = min(abs(found.a), far);
        // Wide enough that every texel of the small map takes several rays, so none is left empty and lets
        // light through a wall it never saw.
        float sharp = pow(facing, 10.0);
        depth += float2(reach, reach * reach) * sharp;
        depthWeights += sharp;
        backs += found.a < 0.0 ? 1u : 0u;
    }
    int2 texel = tile * int(DDGI_TEXELS) + int2(thread.xy);
    float keep = fresh ? 0.0 : DDGI_KEEP;
    float4 lastLight = storageImagesRgba16f[push.textures.y][texel];
    float4 lastDepth = storageImagesRgba16f[push.textures.z][texel];
    float3 freshLight = lightWeights > 0.0 ? light / lightWeights : 0.0;
    float2 freshDepth = depthWeights > 1e-3 ? depth / depthWeights : (fresh ? float2(far, far * far) : lastDepth.xy);
    // Light that has really fallen, as when a lamp goes out, is let go of at once: each ray takes some light from
    // what the probes held last, so light bounced round a room would otherwise feed itself and linger for seconds
    // after its lamp went out, where real light dies at once. Light that rises is smoothed as ever: taken at once,
    // a rise fed itself the same way, and the corridor round a neon tube surged to twice as bright for frames.
    float3 lastLinear = ddgiDecode(lastLight.rgb);
    float was = max(max(lastLinear.r, lastLinear.g), lastLinear.b);
    float now = max(max(freshLight.r, freshLight.g), freshLight.b);
    float trend = lerp(fresh ? 0.0 : lastLight.a, (now - was) / max(max(now, was), 1e-3), DDGI_TREND);
    keep *= 1.0 - smoothstep(DDGI_FALL.x, DDGI_FALL.y, -trend);
    // The running change goes in the light's alpha, which nothing else reads.
    storageImagesRgba16f[push.textures.y][texel] = float4(lerp(ddgiEncode(freshLight), lastLight.rgb, keep), trend);
    storageImagesRgba16f[push.textures.z][texel] = float4(lerp(freshDepth, lastDepth.xy, keep), 0.0, 0.0);
    if (all(thread.xy == 0))
    {
        storageImagesRgba16f[push.textures.w][tile] = ddgiRelocate(frame, cascade, tile, fresh);
    }
}

// ---------------------------------------------------------------- Ray-traced reflections

// The farthest a reflection's ray looks, in metres, before it takes the sky.
static const float REFLECTION_REACH = 500.0;

// Where a pixel is in the world, from its depth.
float3 worldPositionAt(uint depthTexture, int2 pixel)
{
    Frame* frame = push.frame;
    float2 uv = (float2(pixel) + 0.5) / float2(frame->viewport);
    float4 world = transform(frame->invViewProj, float4(uv * 2.0 - 1.0, loadDepth(depthTexture, pixel), 1.0));
    return world.xyz / world.w;
}

// Reflections by tracing the scene's triangles, as the GI's rays do, where
// the GI has them: from each pixel smooth enough to reflect, at half
// resolution, its mirror ray into the scene; the surface it meets is lit
// as the GI's rays light theirs, or where it meets nothing, the sky shows.
// Unlike screen-space reflections, they show what is off screen, behind the
// camera and behind other things: a chrome ball mirrors the whole room.
// Writes the colour and 1, as sure as can be. textures: x depth, z the
// surface: normal and roughness.
[shader("fragment")]
float4 rayReflectionsFragment(AoOut input) : SV_Target
{
    Frame* frame = push.frame;
    uint depthTexture = push.textures.x;
    int2 pixel = fullPixel(int2(input.position.xy));
    float depth = loadDepth(depthTexture, pixel);
    float4 surface = textures[NonUniformResourceIndex(push.textures.z)].Load(int3(pixel, 0));
    float roughness = surface.b;
    if (depth <= 0.0 || roughness >= frame->post.reflectionMaxRoughness)
    {
        return 0.0;
    }
    // The surface's own normal, normal map and all, as the depth passes kept it.
    float3 position = worldPositionAt(depthTexture, pixel);
    float3 eye = frame->cameraPosition;
    float3 view = normalizeOr(eye - position, -frame->cameraForward);
    float3 normal = octDecode(surface.rg * 2.0 - 1.0);
    normal = dot(normal, view) < 0.0 ? -normal : normal;
    float3 centre = float3(0.0, 0.0, dot(position - eye, frame->cameraForward));
    float3 direction = reflect(-view, normal);
    // Off the surface by a little more the farther away it is, as its depth is less sure there.
    float3 origin = position + normal * (0.01 + 0.002 * centre.z);
    Hit hit = traceScene(frame, origin, direction, REFLECTION_REACH, false);
    bool atmosphere = frame->lighting.sky == SKY_ATMOSPHERE;
    // Alpha is 1 and how far the ray went, so Reflections temporal finds last frame's reflection where what it
    // shows was then, not where the mirror was; screen-space reflections write at most 1 there.
    if (!hit.found)
    {
        return float4(surroundingsReflection(frame, position, normal, direction, 0.0, 0.0, atmosphere), 1.0 + REFLECTION_REACH);
    }
    bool back;
    // Sharper textures near, blurrier far, about as the reflection shows them.
    float mip = clamp(log2(1.0 + hit.t * 0.5), 0.0, 4.0);
    float3 at = origin + direction * hit.t;
    float3 light = ddgiShadeHit(frame, hit.instance, hit.primitive, hit.barycentrics, at, direction, mip, back);
    return float4(back ? 0.0 : light, 1.0 + hit.t);
}
