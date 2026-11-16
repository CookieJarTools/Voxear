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

// The probes need no light under water, and compiling it here stops Slang with an internal error.
#define NO_UNDERWATER_LIGHT
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

// Whether a lightmap bake is tracing (light_bake.slang): what neither shades nor bounces light in it is passed
// through, as a cut-out instance's holes are.
static bool bakeTracing = false;

bool skippedByBake(uint id)
{
    return bakeTracing && (buffers()->instances[id].flags & INSTANCE_NO_BAKE_CAST) != 0;
}

// Whether a material is cut out: drawn only where its picture's alpha
// reaches its cutoff, as a card of leaves or a chain-link fence. While a bake
// traces, an instance it leaves out is taken for one, so its rays are tested
// against it and pass through.
bool cutOutMaterial(uint id)
{
    return buffers()->materials[buffers()->instances[id].material].alphaCutoff > 0.0 || skippedByBake(id);
}

// Whether a ray passes through a cut-out instance's triangle where it meets
// it, through the holes its picture leaves, as the depth pass discards
// them. Without this a card of leaves stopped every ray as a solid sheet,
// and the shade under a tree was a dark square.
bool passesThrough(uint id, uint primitive, float2 barycentrics)
{
    if (skippedByBake(id))
    {
        return true;
    }
    Instance instance = buffers()->instances[id];
    Material material = buffers()->materials[instance.material];
    Mesh mesh = buffers()->meshes[instance.mesh];
    uint first = mesh.lods[0].indexOffset + primitive * 3;
    float2 a = buffers()->vertices[mesh.vertexOffset + buffers()->indices[first]].uv;
    float2 b = buffers()->vertices[mesh.vertexOffset + buffers()->indices[first + 1]].uv;
    float2 c = buffers()->vertices[mesh.vertexOffset + buffers()->indices[first + 2]].uv;
    float2 uv = a * (1.0 - barycentrics.x - barycentrics.y) + b * barycentrics.x + c * barycentrics.y;
    float alpha = material.baseColor.a
        * textures[NonUniformResourceIndex(material.baseColorTexture)].SampleLevel(samplers[SAMPLER_LINEAR_REPEAT], baseColorUv(material, uv), 0.0).a;
    return alpha < material.alphaCutoff;
}

#if SOFTWARE_RAYS
#include "trace_software.slang"
#else
[[vk::binding(4, 0)]] RaytracingAccelerationStructure scenes[];

// The nearest triangle along a ray, or any at all with `any`, by the GPU's
// ray tracing, through the scene's top level. Cut-out instances are in it
// as not opaque (src/lib.rs), so their triangles come back as candidates,
// each kept only where the ray does not pass through it.
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
    RayQuery<RAY_FLAG_NONE> query;
    query.TraceRayInline(scene, any ? RAY_FLAG_ACCEPT_FIRST_HIT_AND_END_SEARCH : RAY_FLAG_NONE, 0xFF, ray);
    while (query.Proceed())
    {
        bool kept = query.CandidateType() == CANDIDATE_NON_OPAQUE_TRIANGLE
            && !passesThrough(query.CandidateInstanceID(), query.CandidatePrimitiveIndex(),
                query.CandidateTriangleBarycentrics());
        if (kept)
        {
            query.CommitNonOpaqueTriangleHit();
        }
    }
    hit.found = query.CommittedStatus() == COMMITTED_TRIANGLE_HIT;
    if (any)
    {
        return hit;
    }
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

// What a ray met: the surface's normal, turned to face the ray, its colour
// as diffuse light sees it, its shine, its glow, its lightmap UVs, and
// whether the ray met its back, as from inside something.
struct HitSurface
{
    float3 normal;
    float3 albedo;
    float3 f0;
    float metal;
    float rough;
    float3 emissive;
    float2 lightmapUv;
    bool back;
};

HitSurface hitSurface(Frame* frame, uint id, uint primitive, float2 barycentrics, float3 direction, float mip)
{
    Instance instance = buffers()->instances[id];
    Mesh mesh = buffers()->meshes[instance.mesh];
    uint first = mesh.lods[0].indexOffset + primitive * 3;
    uint ia = buffers()->indices[first];
    uint ib = buffers()->indices[first + 1];
    uint ic = buffers()->indices[first + 2];
    Vertex a = buffers()->vertices[mesh.vertexOffset + ia];
    Vertex b = buffers()->vertices[mesh.vertexOffset + ib];
    Vertex c = buffers()->vertices[mesh.vertexOffset + ic];
    float3 weights = float3(1.0 - barycentrics.x - barycentrics.y, barycentrics.x, barycentrics.y);
    float3 local = a.normal * weights.x + b.normal * weights.y + c.normal * weights.z;
    float3 normal = normalize(instance.normal0.xyz * local.x + instance.normal1.xyz * local.y + instance.normal2.xyz * local.z);
    // Its normals, not its winding, say which side is its front. A sheet, a cut-out card of leaves or a plant,
    // has no inside: its back is no sign of a probe within a wall, and probes in a tree's crown were set aside.
    // A terrain's or a world of blocks' mesh for rays says its surfaces' materials point by point (src/mesh.rs's
    // PROXY_MATERIAL). It is solid ground all the same, a crown of leaf blocks too: its colour is its surface's,
    // but it is no sheet, whatever its surface's picture cuts out.
    bool painted = (instance.flags & INSTANCE_RAYS_ONLY) != 0 && (a.wind & PROXY_MATERIAL) != 0;
    Material material = buffers()->materials[painted ? a.wind & ~PROXY_MATERIAL : instance.material];
    bool sheet = !painted && (material.alphaCutoff > 0.0 || (material.flags & MATERIAL_FOLIAGE) != 0);
    HitSurface hit;
    hit.back = dot(normal, direction) > 0.0 && !sheet;
    // Both faces of a surface take light, as its front does.
    hit.normal = dot(normal, direction) > 0.0 ? -normal : normal;
    float2 uv = a.uv * weights.x + b.uv * weights.y + c.uv * weights.z;
    float4 base = material.baseColor
        * textures[NonUniformResourceIndex(material.baseColorTexture)].SampleLevel(samplers[SAMPLER_LINEAR_REPEAT], uv, mip);
    // Metalness as shading has it: the material's factor times its texture's blue, which glTF factors leave at 1.
    float metal = material.metallic
        * textures[NonUniformResourceIndex(material.metallicRoughnessTexture)].SampleLevel(samplers[SAMPLER_LINEAR_REPEAT], uv, mip).b;
    // As the season has it, so bounced light and reflections turn with the year too.
    float rough = material.roughness;
    seasonalSurface(frame, material, instance.model.c3.xyz, hit.normal, base.rgb, rough);
    hit.albedo = base.rgb * (1.0 - metal);
    hit.f0 = lerp(float3(0.04), base.rgb, metal);
    hit.metal = metal;
    hit.rough = rough;
    hit.emissive = material.emissive
        * textures[NonUniformResourceIndex(material.emissiveTexture)].SampleLevel(samplers[SAMPLER_LINEAR_REPEAT], uv, mip).rgb
        * (bakeTracing ? glowAverage(material) : glowAmount(frame, material, instance.model.c3.xyz));
    hit.lightmapUv = 0.0;
    if (instance.baked != 0)
    {
        float2* uvs = buffers()->lightmapUvs + mesh.vertexOffset;
        hit.lightmapUv = uvs[ia] * weights.x + uvs[ib] * weights.y + uvs[ic] * weights.z;
    }
    return hit;
}

// The light a surface sends back along a ray that hit it, where it hit, and
// whether the ray found the surface's back, as from inside something. For a
// GI probe's ray, its diffuse light alone; for a reflection's or a reflection
// probe's, its shine too, so metals show what they mirror rather than black.
// Beside a bake, a probe's own rays take only the Realtime lights and their
// bounce, as the bake holds the rest; a reflection's take the bake's light
// where what they meet has some, and leave out of its direct light what its
// lightmap holds.
float3 ddgiShadeHit(Frame* frame, uint id, uint primitive, float2 barycentrics,
    float3 position, float3 direction, float mip, bool fromProbe, out bool back)
{
    HitSurface hit = hitSurface(frame, id, primitive, barycentrics, direction, mip);
    back = hit.back;
    float3 normal = hit.normal;
    float3 albedo = hit.albedo;
    BakedInstance baked;
    bool baking = frame->bake.source != GI_REALTIME;
    bool hasBaked = baking && bakedInstance(id, baked);
    bool ownLightmap = hasBaked && lightmapped(baked);
    bool beside = fromProbe && frame->bake.source == GI_BOTH;
    uint firstLight = beside ? frame->bake.bakeLights : (ownLightmap ? frame->bake.bakedLights : 0);
    bool sunLit = beside ? frame->bake.sunMode == LIGHT_MODE_REALTIME : !(ownLightmap && frame->bake.sunMode == LIGHT_MODE_BAKED);

    float3 toSun = -frame->sunDirection;
    bool atmosphere = frame->lighting.sky == SKY_ATMOSPHERE;
    float3 sunLight = atmosphere ? sunThroughAtmosphere(frame) : frame->sunColor;
    float facing = dot(normal, toSun);
    // Shine only where it shows: metals, whose colour is all shine, and not the GI's rays, which gather light
    // from all round, nor a near-white dielectric's few percent.
    bool shiny = !fromProbe && hit.metal > 0.05;
    Surface s;
    s.normal = normal;
    s.view = -direction;
    s.diffuse = albedo;
    s.f0 = shiny ? hit.f0 : 0.0;
    // Not sharper than a capture's texels can show: a mirror's highlight would be one bright texel.
    s.roughness = shiny ? max(hit.rough, 0.2) : 1.0;
    float3 lit = 0.0;
    if (sunLit && facing > 0.0 && !ddgiBlocked(frame, position + normal * 0.02, toSun, 1e4))
    {
        float3 sun = sunLight * glassSun(frame, position, normal);
        lit += shiny ? shade(s, toSun, sun) : albedo / PI * sun * facing;
    }
    for (uint i = firstLight; i < frame->lightCount; i++)
    {
        Light lamp = buffers()->lights[i];
        if (distance(lamp.position, position) < lamp.range + 2.0 * max(lamp.size.x, lamp.size.y))
        {
            lit += shadeLocalLight(s, lamp, position, false);
        }
    }
    // Last frame's light round it, so light bounces again.
    float up = normal.y * 0.5 + 0.5;
    float3 sky = atmosphere ? atmosphereAmbient(frame, up, sunLight)
        : gradientSkyLight(frame, up);
    // Looked up a little back along the ray for a probe's own, as shading looks towards the camera; but a
    // reflection's ray comes from the thing reflecting, and back towards a chrome ball the probes round it are
    // inside it or have it in their way, so straight out from the surface instead.
    float3 towards = fromProbe ? -direction : normal;
    bool both = frame->bake.source == GI_BOTH && frame->ddgi.enabled != 0;
    float3 around;
    float3 bakedLight;
    float3 volume;
    if (!fromProbe && hasBaked && bakedIrradiance(baked, hit.lightmapUv, normal, bakedLight))
    {
        around = bakedLight * frame->bake.strength + (both ? ddgiIrradiance(frame, position, normal, towards, 0.0) : 0.0);
    }
    // Under a bake, what has no baked light of its own takes its probes' grid's.
    else if (!fromProbe && probeVolumeLight(frame, position, normal, volume))
    {
        around = volume + (both ? ddgiIrradiance(frame, position, normal, towards, 0.0) : 0.0);
    }
    else if (frame->ddgi.enabled != 0)
    {
        around = frame->bake.source == GI_BOTH ? ddgiIrradiance(frame, position, normal, towards, 0.0) + (beside ? 0.0 : sky)
            : ddgiIrradiance(frame, position, normal, towards, sky);
    }
    else
    {
        // Without the GI, reflections still trace the scene, and what they meet takes the sky's light alone, as
        // shading does then.
        around = sky;
    }
    float3 mirrored = 0.0;
    if (shiny)
    {
        // What it mirrors: the reflection probes round it, as last captured, then the GI's light, then the sky,
        // as a reflection that finds nothing of its own takes them; dimmed by Fresnel as shading's is.
        float3 outward = reflect(direction, normal);
        float nv = saturate(dot(normal, -direction));
        float3 fresnel = hit.f0 + (max(float3(1.0 - hit.rough), hit.f0) - hit.f0) * pow(1.0 - nv, 5.0);
        float3 off = position + normal * 0.05;
        mirrored = fresnel * reflectionFallback(frame, off, off, normal, outward, hit.rough, around, atmosphere, true);
    }
    return lit + albedo * around + mirrored + hit.emissive;
}

// The sky's light from a direction, as the plain sky light has it; a
// gradient sky's picture as it is that way, blurred over about a radian, so
// the GI's probes take its light from where it comes, a bright side of it
// lighting the side of things that faces it.
float3 ddgiSky(Frame* frame, float3 direction)
{
    float up = direction.y * 0.5 + 0.5;
    if (frame->lighting.sky == SKY_ATMOSPHERE)
    {
        return atmosphereAmbient(frame, up, sunThroughAtmosphere(frame));
    }
    if (frame->lighting.skyPicture != 0)
    {
        return skyPictureSeen(frame, direction, 0.7) * GRADIENT_AMBIENT * frame->ambient;
    }
    return gradientSkyLight(frame, up);
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
        // Beside a bake, which holds the sky's light, the probes hold none of it.
        result = float4(frame->bake.source == GI_BOTH ? 0.0 : ddgiSky(frame, direction), 1e4);
    }
    else
    {
        float t = hit.t;
        float3 position = origin + direction * t;
        bool back;
        // Textures blurred well down: a probe's light is an average over many rays anyway.
        float3 light =
            ddgiShadeHit(frame, hit.instance, hit.primitive, hit.barycentrics, position, direction, 3.0, true, back);
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

// How near a reflection's ray may meet the inside of the surface it left
// before it is taken for having started a hair inside it, in metres.
static const float REFLECTION_SELF = 0.05;
// About how many texels across a metre the scene's textures have, to turn
// how wide a pixel's reflection is where it lands into a mip.
static const float REFLECTION_TEXELS_PER_METRE = 512.0;

// The light a reflection's ray finds where `hit` stopped it, its textures
// read at the mip that a pixel `width` metres wide at its start, spreading by
// `spread` radians, covers there (a ray cone, Akenine-Möller et al. 2019).
float3 ddgiShadeReflected(Frame* frame, Hit hit, float3 origin, float3 direction, float width, float spread, out bool back)
{
    float footprint = width + hit.t * spread;
    float mip = clamp(log2(max(footprint * REFLECTION_TEXELS_PER_METRE, 1.0)), 0.0, 8.0);
    float3 at = origin + direction * hit.t;
    return ddgiShadeHit(frame, hit.instance, hit.primitive, hit.barycentrics, at, direction, mip, false, back);
}

// Reflections by tracing the scene's triangles, as the GI's rays do,
// wherever the GPU has ray queries, GI or not: from each pixel smooth enough
// to reflect, at full resolution, its mirror ray into the scene; the surface
// it meets is lit as the GI's rays light theirs (by the sky alone without the
// GI), or where it meets nothing, the reflection probes round the surface or
// the sky show.
// Unlike screen-space reflections, they show what is off screen, behind the
// camera and behind other things: a chrome ball mirrors the whole room.
// Writes the reflection, and 1 where the ray met something, 0 where it went
// out. textures: x depth, z the surface: normal and roughness.
[shader("fragment")]
float4 rayReflectionsFragment(AoOut input) : SV_Target
{
    Frame* frame = push.frame;
    uint depthTexture = push.textures.x;
    int2 pixel = fullPixel(int2(input.position.xy));
    float depth = loadDepth(depthTexture, pixel);
    float4 surface = textures[NonUniformResourceIndex(push.textures.z)].Load(int3(pixel, 0));
    float roughness = surface.b;
    // The surface's own normal, normal map and all, as the depth passes kept it.
    float3 position = worldPositionAt(depthTexture, pixel);
    float3 eye = frame->cameraPosition;
    float3 view = normalizeOr(eye - position, -frame->cameraForward);
    float3 normal = octDecode(surface.rg * 2.0 - 1.0);
    normal = dot(normal, view) < 0.0 ? -normal : normal;
    float3 centre = float3(0.0, 0.0, dot(position - eye, frame->cameraForward));
    float3 direction = reflect(-view, normal);
    // How wide the pixel is on the surface and how fast its reflection turns from it to the next, worked out
    // with its neighbours before any of them stops: a curved mirror shrinks what it shows, and read at full detail
    // the floor's grout and stains broke into dark dots and lines over the chrome ball, crawling as it moved.
    float width = max(length(ddx(position)), length(ddy(position)));
    float spread = max(length(ddx(direction)), length(ddy(direction)));
    if (depth <= 0.0 || roughness >= frame->post.reflectionMaxRoughness)
    {
        return 0.0;
    }
    // Where depth jumps, at an edge, the neighbours lie on another surface and say nothing of this one.
    width = min(width, 0.05 * centre.z);
    spread = min(spread, 0.05);
    // Off the surface by a little more the farther away it is, as its depth is less sure there.
    float3 origin = position + normal * (0.01 + 0.002 * centre.z);
    Hit hit = traceScene(frame, origin, direction, REFLECTION_REACH, false);
    bool atmosphere = frame->lighting.sky == SKY_ATMOSPHERE;
    bool back = false;
    float3 light = 0.0;
    if (hit.found)
    {
        light = ddgiShadeReflected(frame, hit, origin, direction, width, spread, back);
        // Close by and from behind, the ray started a hair inside the surface it leaves, as at a ball's rim where
        // depth is least sure, and met its inside: on past it.
        if (back && hit.t < REFLECTION_SELF)
        {
            origin += direction * (hit.t + 0.01);
            hit = traceScene(frame, origin, direction, REFLECTION_REACH, false);
            light = hit.found ? ddgiShadeReflected(frame, hit, origin, direction, width, spread, back) : 0.0;
        }
    }
    if (!hit.found)
    {
        // Out to the sky, or what the reflection probes round the surface show of it, as rough as the surface.
        float3 around = reflectionAmbient(frame, position + normal * ddgiSpacing(frame, 0), normal, atmosphere);
        return float4(reflectionFallback(frame, position, origin, normal, direction, roughness, around, atmosphere, false), 0.0);
    }
    // The back of anything else shows lit as its front: a curtain or a card of ivy is a sheet, not the outside of
    // something, and black there left dark patches all over a chrome ball.
    return float4(light, 1.0);
}

// ---------------------------------------------------------------- Reflection probes

// A reflection probe's capture (src/reflection_probes.rs): from its middle,
// a ray through each texel of its cube's six faces, against the scene the
// GI's rays see, what it meets lit as theirs is and its textures read as
// wide as the texel spreads; where it meets nothing, the sky. A thread a
// texel, a face for each z. push.textures.xyz: the probe's middle, as
// floats' bits; push.output: the capture's first mip.
// What a reflection probe's ray sees where it crosses the sea's top or a
// Pool's before it meets anything (`far` along it): the water, reflecting
// what is above it by Fresnel, and through it `beyond`, what the ray met,
// dimmed and tinted by the depth it goes through, or the water's own colour
// where it is deep. Water is not among the scene's triangles, so the ray is
// checked against each flat top here.
float3 probeCaptureWater(Frame* frame, float3 origin, float3 direction, float far, float3 beyond, bool atmosphere)
{
    if (direction.y > -1e-4 || (frame->ocean.enabled == 0 && frame->ocean.pools == 0))
    {
        return beyond;
    }
    float nearest = far;
    float3 deep = frame->ocean.deep;
    float3 absorb = frame->ocean.absorb;
    for (uint i = 0; i <= frame->ocean.pools; i++)
    {
        bool sea = i == frame->ocean.pools;
        if (sea && frame->ocean.enabled == 0)
        {
            break;
        }
        WaterBody pool;
        if (!sea)
        {
            pool = buffers()->waterBodies[i];
            if (pool.liquid != LIQUID_WATER)
            {
                continue;
            }
        }
        float top = sea ? frame->ocean.level : pool.toWorld.c3.y;
        float t = (top - origin.y) / direction.y;
        if (origin.y <= top || t <= 0.0 || t >= nearest)
        {
            continue;
        }
        // Water there, of this top: not a Pool's top beyond its sides, nor the sea's in a Dry Area.
        float found;
        float3 taken;
        if (waterSurface(frame, origin + direction * t - float3(0.0, 0.02, 0.0), found, taken) && abs(found - top) < 0.01)
        {
            nearest = t;
            deep = sea ? frame->ocean.deep : pool.deep;
            absorb = taken;
        }
    }
    if (nearest >= far)
    {
        return beyond;
    }
    float3 at = origin + direction * nearest;
    float3 up = float3(0.0, 1.0, 0.0);
    float facing = saturate(-direction.y);
    float fresnel = 0.02 + 0.98 * pow(1.0 - facing, 5.0);
    float3 around = reflectionAmbient(frame, at + up * 0.05, up, atmosphere);
    float3 mirrored = reflectionFallback(frame, at, at, up, reflect(direction, up), 0.05, around, atmosphere, true);
    // Through it to what the ray met, as far below as it is along the ray; the water's own colour, lit from above,
    // for what it takes away.
    float3 through = exp(-absorb * max(far - nearest, 0.0));
    float3 sun = (atmosphere ? sunThroughAtmosphere(frame) : frame->sunColor) * saturate(-frame->sunDirection.y);
    float3 water = deep * (around + sun / PI);
    return lerp(beyond * through + water * (1.0 - through), mirrored, fresnel);
}

[shader("compute")]
[numthreads(8, 8, 1)]
void probeCaptureMain(uint3 id : SV_DispatchThreadID)
{
    Frame* frame = push.frame;
    if (any(id.xy >= push.size))
    {
        return;
    }
    float3 origin = asfloat(push.textures.xyz);
    float3 direction = normalize(probeFaceDirection(id.z, (float2(id.xy) + 0.5) / float2(push.size)));
    bool atmosphere = frame->lighting.sky == SKY_ATMOSPHERE;
    // A texel's width as an angle: a face's quarter turn over its texels.
    float spread = 0.5 * PI / float(push.size.x);
    Hit hit = traceScene(frame, origin, direction, REFLECTION_REACH, false);
    float3 light;
    if (hit.found)
    {
        bool back;
        light = ddgiShadeReflected(frame, hit, origin, direction, 0.0, spread, back);
    }
    else
    {
        light = reflectedSky(frame, direction, atmosphere);
    }
    light = probeCaptureWater(frame, origin, direction, hit.found ? hit.t : REFLECTION_REACH, light, atmosphere);
    // Kept within what half floats hold, and nothing that is not a number, which would spread through the mips.
    light = select(isnan(light) || isinf(light), float3(0.0), min(light, 60000.0));
    storageArraysRgba16f[push.output][id] = float4(light, 1.0);
}

#include "sound_rays.slang"
#include "light_bake.slang"
