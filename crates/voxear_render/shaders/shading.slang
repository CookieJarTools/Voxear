// Forward PBR (glTF metal/rough) for opaque geometry: what the vertex
// shader (forward.slang) and the mesh shaders (meshlets.slang) both make of
// a vertex, and the fragment shaders that shade it. Vertices are pulled from
// the shared vertex buffer.

#pragma once

#include "atmosphere.slang"
#include "common.slang"
#include "custom.slang"
#include "ddgi_sample.slang"
#include "glow.slang"
#include "impostor.slang"
#include "lights.slang"
#include "occlusion.slang"
#include "seasons.slang"
#include "terrain_ground.slang"
#include "wind.slang"

struct VertexOut
{
    float4 position : SV_Position;
    float3 worldPosition : POSITION;
    float3 normal : NORMAL;
    float4 tangent : TANGENT;
    float2 uv : TEXCOORD0;
    nointerpolation uint material : MATERIAL;
    nointerpolation uint lod : LOD;
    nointerpolation uint instance : INSTANCE;
    nointerpolation uint fade : FADE;
    // What the meshlets debug view colours by.
    nointerpolation uint cluster : CLUSTER;
    float4 currentClip : CURRENT_CLIP;
    float4 previousClip : PREVIOUS_CLIP;
    // How far a custom shader's vertex function moved it from where the mesh has it.
    float3 moved : MOVED;
    // How much of it is bound to hidden bones (skinning.slang): what is mostly hidden is left out.
    float hidden : HIDDEN;
};

struct FragmentOut
{
    float4 color : SV_Target0;
    // Screen motion since last frame, in UV units, for TAA and motion blur.
    float2 velocity : SV_Target1;
};

float2 motion(float4 currentClip, float4 previousClip)
{
    return (currentClip.xy / currentClip.w - previousClip.xy / previousClip.w) * 0.5;
}

// Vertex `vertexIndex` of an instance's mesh, drawn at a LOD in a role in
// its fade, with what the meshlets debug view colours it by.
VertexOut forwardVertex(uint instanceIndex, uint lod, uint fade, uint cluster, uint vertexIndex)
{
    Instance instance = buffers()->instances[instanceIndex];
    Mesh mesh = buffers()->meshes[instance.mesh];
    Vertex v = buffers()->vertices[instanceVertices(instance, mesh) + vertexIndex];
    if (isImpostor(instance))
    {
        return impostorVertex(instance, instanceIndex, v.position.xy, cluster);
    }
    float4 world = transform(instance.model, float4(v.position, 1.0));
    // A skinned instance's vertex where last frame's pose had it.
    float3 before = buffers()->vertices[instanceVerticesBefore(instance, mesh) + vertexIndex].position;
    float4 previous = transform(instance.prevModel, float4(before, 1.0));
    // Plants move in the wind, now and as they were a frame ago, for their motion.
    Material material = buffers()->materials[instance.material];
    float3 normal = normalize(instance.normal0.xyz * v.normal.x + instance.normal1.xyz * v.normal.y + instance.normal2.xyz * v.normal.z);
    float height = plantHeight(instance, mesh);
    Frame* frame = push.frame;
    world.xyz += windOffset(frame, material, instance.model.c3.xyz, height, world.xyz, normal, v.wind, frame->time);
    float earlier = frame->time - frame->deltaTime;
    previous.xyz += windOffset(frame, material, instance.prevModel.c3.xyz, height, previous.xyz, normal, v.wind, earlier);
    // A custom shader's vertex function, now and a frame ago.
    float3 moved = 0.0;
    if (material.shader != STANDARD_SHADER)
    {
        float3 level = levelPosition(frame, world.xyz, 4096.0);
        VertexInput now = { world.xyz, normal, v.uv, instance.model.c3.xyz, frame->time, level };
        moved = customVertex(instance.material, now);
        world.xyz += moved;
        float3 levelBefore = levelPosition(frame, previous.xyz, 4096.0);
        VertexInput before = { previous.xyz, normal, v.uv, instance.prevModel.c3.xyz, earlier, levelBefore };
        previous.xyz += customVertex(instance.material, before);
    }

    VertexOut o;
    o.position = transform(push.frame->viewProj, world);
    o.worldPosition = world.xyz;
    o.normal = instance.normal0.xyz * v.normal.x + instance.normal1.xyz * v.normal.y + instance.normal2.xyz * v.normal.z;
    o.tangent = float4(transform(instance.model, float4(v.tangent.xyz, 0.0)).xyz, v.tangent.w);
    o.uv = v.uv;
    o.material = instance.material;
    o.lod = lod;
    o.instance = instanceIndex;
    o.fade = fade;
    o.cluster = cluster;
    o.currentClip = transform(push.frame->unjitteredViewProj, world);
    o.previousClip = transform(push.frame->prevViewProj, previous);
    o.moved = moved;
    o.hidden = float(v.wind >> 24) / 255.0;
    return o;
}

// A corner of a plant's impostor, turned to the camera. Far off, it does
// not sway in the wind; and it moves on screen as the camera does.
VertexOut impostorVertex(Instance instance, uint instanceIndex, float2 corner, uint cluster)
{
    Frame* frame = push.frame;
    ImpostorCorner c = impostorCorner(instance, frame->cameraPosition, corner);
    VertexOut o;
    o.position = transform(frame->viewProj, float4(c.world, 1.0));
    o.worldPosition = c.world;
    o.normal = c.facing;
    o.tangent = 0.0;
    o.uv = c.uv;
    o.material = instance.material;
    o.lod = 0;
    o.instance = instanceIndex;
    o.fade = 0;
    o.cluster = cluster;
    o.currentClip = transform(frame->unjitteredViewProj, float4(c.world, 1.0));
    o.previousClip = transform(frame->prevViewProj, float4(c.world, 1.0));
    o.moved = 0.0;
    o.hidden = 0.0;
    return o;
}

// Which froxel a pixel falls in: screen tile by logarithmic depth slice.
uint clusterAt(Frame* frame, float2 pixel, float viewDepth)
{
    uint2 tile = min(uint2(pixel / frame->tileSize), frame->clusterGrid.xy - 1);
    float slice = log(max(viewDepth, frame->near)) * frame->clusterScale - frame->clusterBias;
    uint z = uint(clamp(slice, 0.0, float(frame->clusterGrid.z - 1)));
    return tile.x + tile.y * frame->clusterGrid.x + z * frame->clusterGrid.x * frame->clusterGrid.y;
}

// Ordered 4x4 dither, for fading between LODs without blending. `turn`
// moves every pixel's threshold on by as many of the 16, wrapping round, so
// the pattern stays an even dither.
float bayer4(uint2 pixel, uint turn = 0)
{
    static const uint MATRIX[16] = { 0, 8, 2, 10, 12, 4, 14, 6, 3, 11, 1, 9, 15, 7, 13, 5 };
    return (float((MATRIX[(pixel.y & 3) * 4 + (pixel.x & 3)] + turn) & 15u) + 0.5) / 16.0;
}

// The incoming LOD covers `fade` of the pixels and the outgoing one the rest;
// and an instance that fades out with distance, as grass does, covers less
// and less of its pixels towards where it is gone.
void ditherLodFade(VertexOut input)
{
    // A skinned part's hidden skin, as a head a first-person camera looks out of, is left out where a camera
    // sees, from where it is a quarter bound to the hidden bones, so the chin above a camera in the neck goes
    // with the head; the shadow passes draw it all, so it still casts its shadow.
    if (input.hidden > 0.25)
    {
        discard;
    }
    float4 fadeOut = buffers()->instances[input.instance].fade;
    if (fadeOut.y > 0.0)
    {
        float distance = length(input.worldPosition - push.frame->cameraPosition);
        float shown = saturate((fadeOut.y - distance) / max(fadeOut.y - fadeOut.x, 1e-3));
        if (bayer4(uint2(input.position.xy)) >= shown)
        {
            discard;
        }
    }
    if (input.fade == 0)
    {
        return;
    }
    float fade = float((buffers()->lodState[input.instance] >> 8) & 0xFFFF) / 65535.0;
    // Under TAA, which jitters the view, the thresholds turn every frame, all 16 in 16 frames, so each pixel shows
    // the incoming LOD `fade` of the time and TAA blends the two. Held still, the pattern stayed where it was, and
    // a chrome ball showed it as a fine checker along its rim and the edges of what it mirrored while it faded. In
    // bit-reversed order, 0, 8, 4, 12, 2..., so every 2, 4 and 8 frames in a row turn as evenly as 16 do: a fade
    // is over in about 15 frames, and TAA weighs the last few most.
    Frame* frame = push.frame;
    uint turn = any(frame->jitter != 0.0) ? reversebits(frame->frameIndex & 15u) >> 28u : 0u;
    bool incoming = bayer4(uint2(input.position.xy), turn) < fade;
    if (incoming != ((input.fade & DRAW_FADING_IN) != 0))
    {
        discard;
    }
}

// Specular anti-aliasing: where the normal turns a lot from one pixel to
// the next, as over a normal map's fine bumps, the surface is drawn rougher by the spread of normals the pixel
// covers. Mirror-smooth there, a highlight or reflection lands on one pixel one frame and its neighbour the next,
// and the gold embroidery on Sponza's banners sparkled as the camera moved. Scaled by the lighting's setting,
// up to the variance they suggest at 0.5; capped, so no surface gets rougher than about 0.42 from it.
float smoothedRoughness(Frame* frame, float3 normal, float roughness)
{
    float strength = frame->post.specularSmoothing;
    float3 dx = ddx(normal);
    float3 dy = ddy(normal);
    float variance = 0.5 * strength * (dot(dx, dx) + dot(dy, dy));
    float alpha = roughness * roughness;
    return sqrt(saturate(alpha + min(2.0 * variance, 0.18)));
}

float3 surfaceNormal(VertexOut input, Material material)
{
    // Facing the camera where a mesh's own normal is missing, as where two opposite normals meet across a face.
    float3 n = normalizeOr(input.normal, normalize(push.frame->cameraPosition - input.worldPosition));
    if ((material.flags & MATERIAL_IMPOSTOR) != 0)
    {
        return impostorNormal(buffers()->instances[input.instance], material, input.uv, n);
    }
    if ((material.flags & MATERIAL_NORMAL_MAP) == 0)
    {
        return n;
    }
    // A tangent along the normal, or none, gives the normal map no direction to bend it in.
    float3 t = input.tangent.xyz - n * dot(n, input.tangent.xyz);
    if (!(dot(t, t) > 1e-12))
    {
        return n;
    }
    t = normalize(t);
    float3 b = cross(n, t) * input.tangent.w;
    // Only X and Y are read: Z follows from them, which lets normal maps ship as two-channel BC5.
    float2 xy = sampleTexture(material.normalTexture, input.uv).xy * 2.0 - 1.0;
    float3 m = float3(xy * material.normalScale, sqrt(saturate(1.0 - dot(xy, xy))));
    return normalizeOr(t * m.x + b * m.y + n * m.z, n);
}

// Which way a surface faces before its normal map, for where snow settles:
// its own normal, or for an impostor, the plant's as pictured, since its
// quad faces the camera.
float3 shapeNormal(VertexOut input, Material material)
{
    if ((material.flags & MATERIAL_IMPOSTOR) != 0)
    {
        return surfaceNormal(input, material);
    }
    return normalizeOr(input.normal, float3(0.0, 1.0, 0.0));
}

static const float2 CASCADE_TILES[CASCADES] = { float2(0.0, 0.0), float2(0.5, 0.0), float2(0.0, 0.5), float2(0.5, 0.5) };

uint cascadeAt(Frame* frame, float3 position)
{
    float depth = dot(position - frame->cameraPosition, frame->cameraForward);
    uint cascade = 0;
    while (cascade < CASCADES && depth > frame->cascadeFar[cascade])
    {
        cascade++;
    }
    return cascade;
}

// Foliage, lit as leaves are: light wrapped round past the side facing
// away from the sun, so a canopy does not go black on its far side; light
// through a leaf seen against the sun, in the leaf's own colour; and a rim
// of the sun's colour round its edges.
float3 foliageLight(Surface s, float3 toSun, float3 sunLight, float shadow)
{
    static const float WRAP = 0.5;
    static const float THROUGH = 0.6;
    static const float RIM = 0.25;
    float nl = dot(s.normal, toSun);
    float wrapped = saturate((nl + WRAP) / (1.0 + WRAP)) - saturate(nl);
    float3 wrap = s.diffuse / PI * sunLight * wrapped;
    // Looking towards the sun through the leaf, from its far side.
    float against = pow(saturate(dot(-s.view, toSun)), 4.0) * saturate(0.3 - nl);
    float3 through = s.diffuse * sunLight * against * THROUGH;
    float rim = pow(1.0 - saturate(dot(s.normal, s.view)), 4.0) * saturate(nl + 0.5);
    return (wrap + through + sunLight * rim * RIM) * shadow;
}

// The colour the sun's shadows take, rather than going grey: the sun's own
// light, tinted, filling in where it does not reach, as light through a
// canopy or off a warm ground does. Without a tint, nothing.
float3 shadowTint(Frame* frame, Surface s, float3 toSun, float3 sunLight, float shadow)
{
    float3 tint = frame->lighting.sunShadowTint;
    if (all(tint <= 0.0))
    {
        return 0.0;
    }
    float lit = saturate(dot(s.normal, toSun)) * shadow;
    return s.diffuse / PI * sunLight * tint * (1.0 - lit);
}

// What a surface reflects of its surroundings: the sky in the mirror
// direction, or with GI the light the probes see that way, blurring
// towards the light from all round (`ambient`) as the surface roughens.
float3 surroundingsReflection(Frame* frame, float3 position, float3 normal, float3 direction, float roughness, float3 ambient, bool atmosphere)
{
    // A surface this rough reflects the light from all round alone, so nothing more need be looked up.
    float blur = saturate(roughness * 1.5);
    if (blur >= 1.0)
    {
        return ambient;
    }
    float3 sky;
    if (atmosphere)
    {
        sky = skyToward(frame, direction) * skyLook(frame);
    }
    else
    {
        float up = direction.y;
        float3 above = lerp(frame->skyHorizon, frame->skyZenith, pow(saturate(up), 0.45));
        float3 below = lerp(frame->skyHorizon, frame->groundColor, pow(saturate(-up), 0.35));
        sky = up >= 0.0 ? above : below;
    }
    // Scaled as the sky's other light is, so a night scene's floor does not mirror a day sky.
    sky *= frame->ambient;
    if (frame->ddgi.enabled != 0)
    {
        // With GI, the light arriving from the mirror direction as the probes see it: blurred, but never the sky
        // through a wall. Looked up a probe's spacing out from the surface, past the thing reflecting: from just
        // off a chrome ball, the probes round it are inside it or have it in their way, and its reflection was
        // smudged dark wherever they fell, smudges that moved as the camera did.
        float3 outside = position + normal * ddgiSpacing(frame, 0);
        sky = ddgiIrradianceAlong(frame, outside, direction, normal, sky);
    }
    return lerp(sky, ambient, blur);
}

// As surroundingsReflection, with screen-space reflections over it where
// they found something and the surface is smooth enough to show them.
// `found` is how much of the result came from those reflections.
float3 environmentReflection(Frame* frame, float2 pixel, float3 position, float3 normal, float3 direction, float roughness, float3 ambient, bool atmosphere, out float found)
{
    found = 0.0;
    float3 reflected = surroundingsReflection(frame, position, normal, direction, roughness, ambient, atmosphere);
    if (frame->post.reflectionTexture == NO_TEXTURE)
    {
        return reflected;
    }
    float2 uv = pixel / float2(frame->viewport);
    float4 screen = textures[NonUniformResourceIndex(frame->post.reflectionTexture)].SampleLevel(samplers[SAMPLER_LINEAR_CLAMP], uv, 0.0);
    // Up to half the roughness past which they stop, the reflection found on screen or by rays alone; fading to
    // the surroundings over the rest. Blended in all the way down, a little of the surroundings' blotchier light
    // showed through a chrome ball's mirror as dark smudges.
    float smooth = saturate(2.0 - 2.0 * roughness / frame->post.reflectionMaxRoughness);
    found = saturate(screen.a) * smooth;
    return lerp(reflected, screen.rgb, found);
}

// Where a point lies in the sun's shadow map: its cascade's tile of the
// atlas, its place in the tile, and its depth from the sun. `found` is false
// beyond the cascades, or without a shadow map.
struct ShadowLookup
{
    bool found;
    uint cascade;
    float2 uv;
    float depth;
    float2 low;
    float2 high;
};

ShadowLookup shadowLookup(Frame* frame, float3 position, float3 normal)
{
    ShadowLookup lookup;
    lookup.found = false;
    uint cascade = cascadeAt(frame, position);
    if (frame->shadowTexture == NO_TEXTURE || cascade >= CASCADES)
    {
        return lookup;
    }
    float3 offsetPosition = position + normal * frame->cascadeTexel[cascade] * 1.5;
    float4 clip = transform(frame->cascadeViewProj[cascade], float4(offsetPosition, 1.0));
    float2 uv = clip.xy * 0.5 + 0.5;
    if (any(uv < 0.0) || any(uv > 1.0) || clip.z > 1.0)
    {
        return lookup;
    }
    float texel = 0.5 / float(push.size.x);
    float2 tile = CASCADE_TILES[cascade];
    lookup.found = true;
    lookup.cascade = cascade;
    lookup.uv = tile + uv * 0.5;
    lookup.depth = clip.z;
    lookup.low = tile + texel * 1.5;
    lookup.high = tile + 0.5 - texel * 1.5;
    return lookup;
}

// 3x3 taps of hardware 2x2 PCF across one cascade's tile of the atlas: the
// sun's shadow with a hard edge, as the volumetric fog takes it.
float sunShadowHard(Frame* frame, float3 position, float3 normal)
{
    // Under the clouds' shadow too, wherever the point is.
    float clouds = cloudShadowAt(frame, position);
    ShadowLookup lookup = shadowLookup(frame, position, normal);
    if (!lookup.found)
    {
        return clouds;
    }
    float texel = 0.5 / float(push.size.x);
    float lit = 0.0;
    for (int y = -1; y <= 1; y++)
    {
        for (int x = -1; x <= 1; x++)
        {
            float2 atlasUv = clamp(lookup.uv + float2(x, y) * texel * 2.0, lookup.low, lookup.high);
            lit += textures[frame->shadowTexture].SampleCmpLevelZero(comparisonSamplers[COMPARISON_SAMPLER_SHADOW], atlasUv, lookup.depth);
        }
    }
    return lit / 9.0 * clouds;
}

// Points on a disc spread evenly by the golden angle, turned by `angle`.
float2 vogelDisc(uint index, uint count, float angle)
{
    float radius = sqrt((float(index) + 0.5) / float(count));
    float theta = float(index) * 2.39996323 + angle;
    return radius * float2(cos(theta), sin(theta));
}

// Few taps, turned each frame and pixel, as TAA averages them.
static const uint PCSS_BLOCKER_TAPS = 12;
static const uint PCSS_FILTER_TAPS = 16;
// The farthest, in metres, a blocker is looked for above what it shades.
static const float PCSS_SEARCH_METRES = 8.0;
// The widest a shadow's soft edge grows, in metres. The taps are the same
// whatever its width, so a wide edge costs no more, only spreads them.
static const float PCSS_MAX_METRES = 1.5;

// The sun's shadow, soft as the sun's disc makes it: percentage-closer soft
// shadows (Fernando 2005). Blockers are found round the point first; the
// farther they are above it, the wider the filter, so shadows stay sharp
// where things touch and soften away from them. Without a softness it is
// the hard shadow.
float sunShadow(Frame* frame, float3 position, float3 normal)
{
    float tanHalfAngle = frame->lighting.sunSoftness;
    if (tanHalfAngle <= 0.0)
    {
        return sunShadowHard(frame, position, normal);
    }
    float clouds = cloudShadowAt(frame, position);
    ShadowLookup lookup = shadowLookup(frame, position, normal);
    if (!lookup.found)
    {
        return clouds;
    }
    float texelUv = 0.5 / float(push.size.x);
    float worldTexel = frame->cascadeTexel[lookup.cascade];
    // The shadow map's depth runs over its cascade's diameter, as wide as all its texels.
    float depthMetres = worldTexel * float(push.size.x);
    float angle = frac(52.9829189 * frac(dot(position.xz * 37.0, float2(0.06711056, 0.00583715)))) * 2.0 * PI;
    angle += float(frame->frameIndex % 64) * 2.39996323;

    float maxTexels = PCSS_MAX_METRES / worldTexel;
    float searchTexels = clamp(PCSS_SEARCH_METRES * tanHalfAngle / worldTexel, 1.0, maxTexels);
    Texture2D atlas = textures[frame->shadowTexture];
    float blockers = 0.0;
    float blockerDepth = 0.0;
    for (uint i = 0; i < PCSS_BLOCKER_TAPS; i++)
    {
        float2 atlasUv = clamp(lookup.uv + vogelDisc(i, PCSS_BLOCKER_TAPS, angle) * searchTexels * texelUv * 2.0, lookup.low, lookup.high);
        float depth = atlas.SampleLevel(samplers[SAMPLER_NEAREST_CLAMP], atlasUv, 0.0).r;
        if (depth < lookup.depth)
        {
            blockers += 1.0;
            blockerDepth += depth;
        }
    }
    // Nothing round it is above it, or everything is: wholly lit or wholly shaded, with no edge to soften.
    if (blockers == 0.0)
    {
        return clouds;
    }
    if (blockers == float(PCSS_BLOCKER_TAPS))
    {
        return 0.0;
    }
    float above = (lookup.depth - blockerDepth / blockers) * depthMetres;
    float filterTexels = clamp(above * tanHalfAngle * 2.0 / worldTexel, 1.0, maxTexels);
    float lit = 0.0;
    for (uint i = 0; i < PCSS_FILTER_TAPS; i++)
    {
        float2 atlasUv = clamp(lookup.uv + vogelDisc(i, PCSS_FILTER_TAPS, angle) * filterTexels * texelUv * 2.0, lookup.low, lookup.high);
        lit += atlas.SampleCmpLevelZero(comparisonSamplers[COMPARISON_SAMPLER_SHADOW], atlasUv, lookup.depth);
    }
    return lit / float(PCSS_FILTER_TAPS) * clouds;
}

// The shadow atlas's cascades, each this many texels across (src/shadows.rs).
static const float CASCADE_RESOLUTION = 2048.0;

// What glass lets through of the sun to a point (glassLight), white where
// no glass is in the way.
float3 glassSun(Frame* frame, float3 position, float3 normal)
{
    if (frame->glassShadow.x == 0)
    {
        return 1.0;
    }
    ShadowLookup lookup = shadowLookup(frame, position, normal);
    if (!lookup.found)
    {
        return 1.0;
    }
    // The sun's part is the atlas's top square, its cascades laid out as the shadow atlas's.
    float2 scale = float2(1.0, float(GLASS_SUN_SIZE) / float(GLASS_SHADOW_ATLAS.y));
    float2 tile = CASCADE_TILES[lookup.cascade] * scale;
    float shadowTexel = frame->cascadeTexel[lookup.cascade];
    float texelMetres = shadowTexel * CASCADE_RESOLUTION / (0.5 * float(GLASS_SUN_SIZE));
    float depthMetres = shadowTexel * CASCADE_RESOLUTION;
    return glassLight(frame, lookup.uv * scale, tile, tile + 0.5 * scale, texelMetres, lookup.depth, depthMetres);
}

float3 debugColor(uint index)
{
    static const float3 PALETTE[6] = {
        float3(0.20, 0.80, 0.30), float3(0.95, 0.85, 0.20), float3(0.95, 0.50, 0.15),
        float3(0.90, 0.20, 0.20), float3(0.70, 0.25, 0.85), float3(0.25, 0.45, 0.95),
    };
    return PALETTE[min(index, 5)];
}

// A mesh's edges over the lit scene, for the lit wireframe view: drawn as
// lines by the edges pipeline, faint enough for the shading to read.
[shader("fragment")]
float4 edgeFragment(VertexOut input) : SV_Target0
{
    ditherLodFade(input);
    return float4(0.0, 0.0, 0.0, 0.35);
}

// The light falling on a surface from all round: from the GI's probes,
// or the hemisphere's sky light where GI is off or beyond its probes.
float3 ambientLight(Frame* frame, float3 position, Surface s, float3 sunLight, bool atmosphere)
{
    float up = s.normal.y * 0.5 + 0.5;
    float3 ambient = atmosphere ? atmosphereAmbient(frame, up, sunLight)
        : lerp(frame->groundColor, lerp(frame->skyHorizon, frame->skyZenith, 0.5), up) * 0.35 * frame->ambient;
    if (frame->ddgi.enabled != 0)
    {
        ambient = ddgiIrradiance(frame, position, s.normal, s.view, ambient);
    }
    return ambient;
}

// The light the lamps in a pixel's cluster send off a surface.
float3 localLights(Frame* frame, Surface s, float3 position, float2 pixel)
{
    float viewDepth = dot(position - frame->cameraPosition, frame->cameraForward);
    uint cluster = clusterAt(frame, pixel, viewDepth);
    uint clusterBase = cluster * (frame->clusterMaxLights + 1);
    uint clusterLights = buffers()->clusters[clusterBase];
    float3 color = 0.0;
    for (uint i = 0; i < clusterLights; i++)
    {
        color += shadeLocalLight(s, buffers()->lights[buffers()->clusters[clusterBase + 1 + i]], position);
    }
    return color;
}

// How far `world` is outside Dry Area `body`, negative inside.
float dryDistance(WaterBody body, float3 world)
{
    // A Wind Zone kept among the Dry Areas keeps no water out.
    if (body.kind != WATER_DRY)
    {
        return 1.0;
    }
    // A mesh's inside is known from where it starts and ends on screen (inDryMesh).
    if (body.shape == DRY_MESH)
    {
        return 1.0;
    }
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

// Whether `world` is inside a Dry Area shaped as a mesh: past the nearest of
// their faces towards the camera along its view and short of the farthest
// away (water.slang's dryMeshFragment), give or take a few centimetres, so
// a mesh's own outside is not in it and its inside floor is. At the camera
// itself, by the middle of the near plane; off the screen, nowhere.
bool inDryMesh(Frame* frame, float3 world)
{
    if (frame->ocean.dryMask == NO_TEXTURE)
    {
        return false;
    }
    float4 clip = transform(frame->viewProj, float4(world, 1.0));
    float2 uv = clip.w > frame->near ? clip.xy / clip.w * 0.5 + 0.5 : 0.5;
    if (any(uv < 0.0) || any(uv > 1.0))
    {
        return false;
    }
    float2 mask = textures[NonUniformResourceIndex(frame->ocean.dryMask)].Load(int3(int2(uv * float2(frame->viewport)), 0)).rg;
    if (mask.y <= 0.0)
    {
        return false;
    }
    float along = max(clip.w, frame->near);
    float front = mask.x > 0.0 ? frame->near / mask.x : 0.0;
    float back = frame->near / max(1.0 - mask.y, 1e-7);
    return along > front + 0.05 && along < back + 0.05;
}

// Whether a bilge, Pool `pool`, holds water at `world`: inside the Dry Area
// it is in (its shape, less one), a little in from its sides.
bool inBilge(Frame* frame, WaterBody pool, float3 world)
{
    WaterBody dry = buffers()->waterBodies[pool.shape - 1];
    return dry.shape == DRY_MESH ? inDryMesh(frame, world) : dryDistance(dry, world) < -0.02;
}

// Whether `world` is over Pool `pool`'s footprint and above its bottom.
bool overPool(WaterBody pool, float3 world)
{
    float3 local = transform(pool.toLocal, float4(world, 1.0)).xyz;
    return all(abs(local.xz) <= pool.size.xz * 0.5) && local.y > -pool.size.y;
}

// The water whose surface is over or under `world`: how high that surface
// is and how fast its water takes each colour, a metre; the sea at its
// level, or a Pool over its footprint above its bottom, not in a Dry Area
// unless it is that Dry Area's bilge. False where there is none.
// A world of blocks' water round the camera (WATER_BLOCKS) is found only for
// `seen`, where a view starts: it is the stretch of that water the camera is
// in, so the ground it lit or wetted would change where that stretch ends
// and as the camera came near.
bool waterSurface(Frame* frame, float3 world, out float surface, out float3 absorb, bool seen = false)
{
    surface = -1e9;
    absorb = frame->ocean.absorb;
    if (frame->ocean.enabled == 0 && frame->ocean.pools == 0)
    {
        return false;
    }
    uint pools = frame->ocean.pools;
    for (uint i = 0; i < pools; i++)
    {
        WaterBody pool = buffers()->waterBodies[i];
        if (pool.shape != 0 && overPool(pool, world) && inBilge(frame, pool, world))
        {
            surface = pool.toWorld.c3.y;
            absorb = pool.absorb;
            return true;
        }
    }
    uint dries = pools + frame->ocean.streams;
    for (uint i = dries; i < dries + frame->ocean.dries; i++)
    {
        if (dryDistance(buffers()->waterBodies[i], world) < 0.0)
        {
            return false;
        }
    }
    if (inDryMesh(frame, world))
    {
        return false;
    }
    if (frame->ocean.enabled != 0)
    {
        surface = frame->ocean.level;
    }
    for (uint i = 0; i < pools; i++)
    {
        WaterBody pool = buffers()->waterBodies[i];
        // Lava is no water to be under.
        bool counted = seen || pool.kind != WATER_BLOCKS;
        if (counted && pool.liquid == LIQUID_WATER && pool.shape == 0 && overPool(pool, world))
        {
            surface = pool.toWorld.c3.y;
            absorb = pool.absorb;
        }
    }
    return surface > -1e8;
}

// The water over `world`, if it is under any (see waterSurface).
bool waterOver(Frame* frame, float3 world, out float surface, out float3 absorb)
{
    if (!waterSurface(frame, world, surface, absorb))
    {
        return false;
    }
    return world.y < surface;
}

// The caustics round the camera (src/water.rs's CAUSTIC_TEXELS and
// CAUSTIC_SPAN): this many texels a side, across this many metres; and the
// two depths under the surface they are worked out at, in metres.
static const uint CAUSTIC_TEXELS = 1024;
static const float CAUSTIC_SPAN = 24.0;
static const float CAUSTIC_NEAR = 1.5;
static const float CAUSTIC_FAR = 6.0;

// How bright the sunlight is `depth` metres under the water where its way
// down crossed the surface at `crossed`, against what flat water lets
// through: 1 on average, brighter where the waves over it focus it and
// darker where they spread it (water.slang's causticsMain). As soft as a
// pixel spans, `across` and `down` metres of `crossed` from it to the
// next, and a little softer the deeper; plain 1 deep down and off the edge
// of the caustics round the camera.
float causticsAt(Frame* frame, float2 crossed, float depth, float2 across, float2 down)
{
    if (frame->ocean.caustics == NO_TEXTURE || depth <= 0.0)
    {
        return 1.0;
    }
    float2 uv = (crossed - frame->ocean.causticOrigin) / CAUSTIC_SPAN;
    float2 edge = min(uv, 1.0 - uv);
    float inside = saturate(min(edge.x, edge.y) / 0.12);
    if (inside <= 0.0)
    {
        return 1.0;
    }
    float soft = (1.0 + depth * 0.05) / CAUSTIC_SPAN;
    Texture2D caustics = textures[NonUniformResourceIndex(frame->ocean.caustics)];
    float2 both = caustics.SampleGrad(samplers[SAMPLER_ANISOTROPIC_CLAMP], uv, across * soft, down * soft).xy;
    float near = lerp(1.0, both.x, saturate(depth / CAUSTIC_NEAR));
    float far = lerp(both.x, both.y, saturate((depth - CAUSTIC_NEAR) / (CAUSTIC_FAR - CAUSTIC_NEAR)));
    float bright = depth < CAUSTIC_NEAR ? near : far;
    bright = lerp(1.0, bright, exp(-max(depth - CAUSTIC_FAR, 0.0) / 12.0));
    return lerp(1.0, bright, inside);
}

// The water over `world`, for the light coming down to it: whether it is
// under water, how high the surface is and how the water takes each colour.
struct WaterOver
{
    bool under;
    float surface;
    float3 absorb;
};

WaterOver waterOverAt(Frame* frame, float3 world)
{
    WaterOver over;
#ifdef NO_UNDERWATER_LIGHT
    over.under = false;
    over.surface = -1e9;
    over.absorb = 0.0;
#else
    over.under = waterOver(frame, world, over.surface, over.absorb);
#endif
    return over;
}

// How much of the sea's waves reach `xz`, from 1 out at sea to 0 where the
// shallows have calmed them (water.slang's seaFloorAt, from the sea floor
// round the camera); 1 beyond it.
float wavesHere(Frame* frame, float2 xz)
{
    static const uint SLICES = 4;
    Texture2D depths = textures[NonUniformResourceIndex(frame->ocean.floorDepth)];
    for (uint i = 0; i < SLICES; i++)
    {
        float4 slice = frame->ocean.floor[i];
        float2 uv = (xz - slice.xy) / slice.z;
        if (slice.w <= 0.0 || any(uv < 0.02) || any(uv > 0.98))
        {
            continue;
        }
        return depths.SampleLevel(samplers[SAMPLER_LINEAR_CLAMP], (float2(float(i % 2), float(i / 2)) + uv) * 0.5, 0.0).g;
    }
    return 1.0;
}

// A smooth noise over `p`, from 0 to 1, blotches a unit across.
float wetNoise(float2 p)
{
    float2 cell = floor(p);
    float2 f = p - cell;
    f = f * f * (3.0 - 2.0 * f);
    float a = frac(sin(dot(cell, float2(127.1, 311.7))) * 43758.5453);
    float b = frac(sin(dot(cell + float2(1.0, 0.0), float2(127.1, 311.7))) * 43758.5453);
    float c = frac(sin(dot(cell + float2(0.0, 1.0), float2(127.1, 311.7))) * 43758.5453);
    float d = frac(sin(dot(cell + float2(1.0, 1.0), float2(127.1, 311.7))) * 43758.5453);
    return lerp(lerp(a, b, f.x), lerp(c, d, f.x), f.y);
}

// What the weather has left on a surface at `world`, facing `normal` (src/weather.rs): wet from the rain, darker
// and glossier, what faces up most; and snow settled on what faces up, in patches while there is little of it.
// It lies under roofs too: nothing here knows what is over a surface.
void weatherSurface(Frame* frame, float3 world, float3 normal, inout float3 albedo, inout float roughness)
{
    float wet = frame->seasons.weatherWet;
    float snow = frame->seasons.weatherSnow;
    if (wet <= 0.0 && snow <= 0.0)
    {
        return;
    }
    float up = saturate(normal.y * 0.5 + 0.5);
    wet *= 0.5 + 0.5 * up;
    albedo *= 1.0 - 0.42 * wet;
    roughness *= 1.0 - 0.6 * wet;
    if (snow > 0.0)
    {
        // Patches a few metres across take it first, then the rest; hashed on whole numbers (clouds.slang).
        float2 at = levelPosition(frame, world, 4096.0).xz;
        float patches = cloudPatches(at * 0.31) * 0.65 + cloudPatches(at * 1.9 + 17.0) * 0.35;
        float lying = saturate((snow * 1.6 - patches) / 0.25) * smoothstep(0.35, 0.75, normal.y);
        albedo = lerp(albedo, float3(0.85, 0.87, 0.9), lying);
        roughness = lerp(roughness, 0.6, lying);
    }
}

// Wet where the water washes over a surface at `world`, facing `normal`:
// from the waterline up as high as the waves reach, its top uneven, with a
// few drips run down a wall from higher; darker and glossier, `albedo` and
// `roughness`, and a little darker under the water too. None in a Dry Area,
// or without the Ocean's Wet Edges.
void waterWetSurface(Frame* frame, float3 world, float3 normal, inout float3 albedo, inout float roughness)
{
#ifndef NO_UNDERWATER_LIGHT
    static const uint WET_EDGES = 256;
    float surface;
    float3 absorb;
    if ((frame->ocean.features & WET_EDGES) == 0 || !waterSurface(frame, world, surface, absorb))
    {
        return;
    }
    float above = world.y - surface;
    bool sea = frame->ocean.enabled != 0 && surface == frame->ocean.level;
    float reach = sea ? 0.12 + min(0.3 * frame->ocean.waveHeight * frame->ocean.waveScale * wavesHere(frame, world.xz), 0.9) : 0.12;
    if (above > reach * 1.6)
    {
        return;
    }
    // Along a wall the noise runs round it; over the ground, across it. Read from the level's middle, so the wet
    // line stays where it is as the world's middle moves.
    float3 level = levelPosition(frame, world, 4096.0);
    float2 along = abs(normal.y) < 0.7 ? float2(level.x + level.z, level.y * 0.3) : level.xz;
    float uneven = 0.7 + 0.5 * wetNoise(along * 2.0) + 0.15 * wetNoise(along * 7.0);
    // A drip down a few narrow columns of a wall, each its own length.
    float across = (level.x + level.z) * 18.0;
    float column = frac(sin(floor(across) * 12.9898) * 43758.5453);
    float drips = abs(normal.y) < 0.5 && column > 0.82 ? (column - 0.82) * 4.0 * pow(sin(frac(across) * PI), 4.0) : 0.0;
    float top = reach * (uneven + drips);
    float wet = 1.0 - smoothstep(top * 0.7, top, above);
    float darker = above < 0.0 ? 0.25 : 0.4;
    albedo *= 1.0 - darker * wet;
    roughness *= 1.0 - 0.6 * wet;
#endif
}

// The terrain under `world`, for a model's foot blended into it: how high its ground is there and which way it faces,
// and its look there from above, its surfaces' (or for one with none, its material's) colour, metallic, roughness
// and normal. False where no terrain reaches, or in a hole.
bool groundUnder(float3 world, float3 dx, float3 dy, out float height, out TerrainLayer look)
{
    look.base = float4(0.0);
    look.normal = float3(0.0, 1.0, 0.0);
    look.metallic = 0.0;
    look.roughness = 1.0;
    height = 0.0;
    for (uint i = 0; i < MAX_TERRAINS; i++)
    {
        Terrain terrain = buffers()->terrains[i];
        if (terrain.tiles.x == 0)
        {
            return false;
        }
        float2 local = world.xz - terrain.corner.xz;
        float2 size = float2(terrain.tiles * TERRAIN_TILE_QUADS) * terrain.spacing;
        if (any(local < 0.0) || any(local > size) || terrainCut(terrain, local))
        {
            continue;
        }
        height = terrain.corner.y + terrainHeight(terrain, local);
        float w = terrain.spacing;
        float slopeX = terrainHeight(terrain, local + float2(w, 0.0)) - terrainHeight(terrain, local - float2(w, 0.0));
        float slopeZ = terrainHeight(terrain, local + float2(0.0, w)) - terrainHeight(terrain, local - float2(0.0, w));
        float3 ground = normalize(float3(-slopeX, 2.0 * w, -slopeZ));
        TerrainSpot spot;
        spot.place = float3(local.x, height - terrain.corner.y, local.y);
        spot.dx = dx;
        spot.dy = dy;
        float4 tangent = float4(1.0, 0.0, 0.0, 1.0);
        if (terrain.surfaceCount == 0)
        {
            TerrainSurface plain;
            plain.material = terrain.material;
            plain.uvScale = terrain.uvScale;
            plain.sharpness = 0.0;
            plain.flags = 0;
            plain.tracks = 0.0;
            plain.fillsIn = 0.0;
            look = terrainLayerAt(plain, spot, ground, tangent);
            return true;
        }
        uint4 ids;
        float4 shares;
        terrainShares(terrain, local, ids, shares);
        float3 bent = float3(0.0);
        float used = 0.0;
        for (uint k = 0; k < 4; k++)
        {
            if (shares[k] < 0.004)
            {
                continue;
            }
            TerrainLayer layer = terrainLayerAt(terrain.surfaces[min(ids[k], terrain.surfaceCount - 1)], spot, ground, tangent);
            look.base += layer.base * shares[k];
            look.metallic += layer.metallic * shares[k];
            look.roughness += layer.roughness * shares[k];
            bent += layer.normal * shares[k];
            used += shares[k];
        }
        used = max(used, 1e-5);
        look.base /= used;
        look.metallic /= used;
        look.roughness = look.roughness / used;
        look.normal = normalizeOr(bent, ground);
        return true;
    }
    return false;
}

// A model's foot blended into the terrain under it, for a material with a ground blend: from where it meets the
// ground up as many metres as the blend says, its edge wandering, it takes the ground's own look there, as earth and
// snow creep up a rock's foot, more on what faces up. All ground where it meets it, facing as the ground faces, so
// no seam shows.
void groundBlend(Frame* frame, Material material, float3 world, inout float4 base, inout float metallic, inout float roughness, inout float3 normal)
{
    if (material.groundBlend <= 0.0)
    {
        return;
    }
    // Before any branch a pixel takes on its own, where the pictures' gradients are defined.
    float3 dx = ddx(world);
    float3 dy = ddy(world);
    float height;
    TerrainLayer ground;
    if (!groundUnder(world, dx, dy, height, ground))
    {
        return;
    }
    // Further up what faces up, and all ground where it meets it, whichever way it faces.
    float3 level = levelPosition(frame, world, 4096.0);
    float reach = material.groundBlend * (0.6 + 0.8 * wetNoise(level.xz * 1.7 + level.y * 0.5));
    reach *= lerp(0.6, 1.4, saturate(normal.y * 0.5 + 0.5));
    float covered = 1.0 - smoothstep(0.0, reach, world.y - height);
    if (covered <= 0.001)
    {
        return;
    }
    base = float4(lerp(base.rgb, ground.base.rgb, covered), base.a);
    metallic = lerp(metallic, ground.metallic, covered);
    roughness = lerp(roughness, ground.roughness, covered);
    normal = normalizeOr(lerp(normal, ground.normal, covered * covered), normal);
}

// How much of a lamp at `lamp` reaches `world` past the water between them,
// dimmed a colour at a time over as much of the way as lies under it: the
// water over `world` (`over`), or the sea's level where `world` is out of
// the water. None of the way counts where `world` is below the sea's level
// but out of it, as in a hull's Dry Area, as the lamp is taken to be too.
float3 lampThroughWater(Frame* frame, float3 lamp, float3 world, WaterOver over)
{
    float surface = over.surface;
    float3 absorb = over.absorb;
    if (!over.under)
    {
        if (frame->ocean.enabled == 0 || world.y < frame->ocean.level)
        {
            return 1.0;
        }
        surface = frame->ocean.level;
        absorb = frame->ocean.absorb;
    }
    float low = min(lamp.y, world.y);
    if (low >= surface)
    {
        return 1.0;
    }
    float high = max(lamp.y, world.y);
    float share = high <= surface ? 1.0 : (surface - low) / (high - low);
    return exp(-absorb * length(lamp - world) * share);
}

// How much of the sun reaches `world` under water, seen in a fragment:
// dimmed a colour at a time by the water it comes down through, and focused
// and spread by the waves into caustics, as sharp as the pixel; and in
// `sky`, how much of the sky's light does. One out of water.
float3 underwaterSun(Frame* frame, float3 world, out float3 sky)
{
    return underwaterSunUnder(frame, world, waterOverAt(frame, world), sky);
}

// As underwaterSun, with the water over `world` already found.
float3 underwaterSunUnder(Frame* frame, float3 world, WaterOver over, out float3 sky)
{
    sky = 1.0;
#ifdef NO_UNDERWATER_LIGHT
    return 1.0;
#else
    float surface = over.surface;
    float3 absorb = over.absorb;
    if (!over.under)
    {
        return 1.0;
    }
    float depth = surface - world.y;
    // The sun's way down, bent at the surface, and how much of the slant it comes down.
    float3 bent = refract(frame->sunDirection, float3(0.0, 1.0, 0.0), 1.0 / 1.33);
    float slant = max(-bent.y, 0.2);
    sky = exp(-absorb * depth);
    // Where its way down crossed the surface.
    float2 crossed = world.xz - bent.xz / slant * depth;
    return exp(-absorb * depth / slant) * causticsAt(frame, crossed, depth, ddx(crossed), ddy(crossed));
#endif
}

// A custom shader's surface, from the standard one's: the material's
// colour, normal, metallic, roughness and emission; `thickness` as glass
// sees through it, or SOLID_THICKNESS.
SurfaceOutput customSurfaceAt(Frame* frame, VertexOut input, SurfaceOutput surface, float3 origin, float thickness, bool glass)
{
    float3 view = normalize(frame->cameraPosition - input.worldPosition);
    float3 level = levelPosition(frame, input.worldPosition, 4096.0);
    SurfaceInput at = { input.uv, input.worldPosition, normalize(input.normal), view, origin, frame->time, thickness, glass, level };
    customSurface(input.material, at, surface);
    surface.normal = normalizeOr(surface.normal, at.normal);
    surface.metallic = saturate(surface.metallic);
    surface.roughness = saturate(surface.roughness);
    surface.alpha = saturate(surface.alpha);
    surface.reflectance = saturate(surface.reflectance);
    surface.transmission = saturate(surface.transmission);
    return surface;
}

// An unlit custom shader's pixel: its colour as it is, hazed by the
// atmosphere as anything lit is.
FragmentOut unlitOutput(Frame* frame, VertexOut input, float3 color)
{
    if (frame->lighting.sky == SKY_ATMOSPHERE)
    {
        float2 screenUv = input.position.xy / float2(frame->viewport);
        float4 haze = aerialPerspectiveAt(frame, screenUv, length(input.worldPosition - frame->cameraPosition));
        color = color * haze.a + haze.rgb * skyLook(frame);
    }
    FragmentOut o;
    o.color = float4(color, 1.0);
    o.velocity = motion(input.currentClip, input.previousClip);
    return o;
}

[shader("fragment")]
FragmentOut fragmentMain(VertexOut input)
{
    Frame* frame = push.frame;
    ditherLodFade(input);
    Material material = buffers()->materials[input.material];

    float4 base = material.baseColor * sampleBaseColor(material, input.uv);
    bool custom = material.shader != STANDARD_SHADER;
    // A custom shader's surface is cut out by its own alpha, below.
    if (!custom && material.alphaCutoff > 0.0 && base.a < material.alphaCutoff)
    {
        discard;
    }
    // A plant's own tint, between its Foliage's healthy and dry colours.
    uint tint = asuint(buffers()->instances[input.instance].fade.z);
    if (tint != 0)
    {
        base.rgb *= float3(tint & 0xFF, (tint >> 8) & 0xFF, (tint >> 16) & 0xFF) / 255.0;
    }
    float4 metalRough = sampleTexture(material.metallicRoughnessTexture, input.uv);
    float metallic = material.metallic * metalRough.b;
    float roughness = material.roughness * metalRough.g;
#ifdef TERRAIN_SHADING
    // A terrain's own surfaces in place of its material's (terrain.slang).
    float3 terrainNormal;
    float3 terrainLight = float3(0.0);
    bool terrainSurfaced = terrainShading(input, base, metallic, roughness, terrainNormal, terrainLight);
#endif
    float3 origin = buffers()->instances[input.instance].model.c3.xyz;
    seasonalSurface(frame, material, origin, shapeNormal(input, material), base.rgb, roughness);
    waterWetSurface(frame, input.worldPosition, shapeNormal(input, material), base.rgb, roughness);
    weatherSurface(frame, input.worldPosition, shapeNormal(input, material), base.rgb, roughness);
    float3 emission =
        material.emissive * sampleTexture(material.emissiveTexture, input.uv).rgb * glowAmount(frame, material, origin);
    float3 normal = surfaceNormal(input, material);
#ifdef TERRAIN_SHADING
    normal = terrainSurfaced ? terrainNormal : normal;
    emission += terrainLight;
#endif
    // A model's foot blended into the ground under it.
    groundBlend(frame, material, input.worldPosition, base, metallic, roughness, normal);
    SurfaceOutput surface = materialSurface(base.rgb, base.a, normal, metallic, roughness, emission);
    if (custom)
    {
        surface = customSurfaceAt(frame, input, surface, origin, SOLID_THICKNESS, false);
        if (material.alphaCutoff > 0.0 && surface.alpha < material.alphaCutoff)
        {
            discard;
        }
        base.rgb = surface.color;
        normal = surface.normal;
        metallic = surface.metallic;
        roughness = surface.roughness;
        emission = surface.emission;
        if (customUnlit(input.material))
        {
            return unlitOutput(frame, input, surface.color + emission);
        }
    }
    bool customLit = custom && customLighting(input.material);

    Surface s;
    s.normal = normal;
    s.view = normalize(frame->cameraPosition - input.worldPosition);
    s.roughness = clamp(smoothedRoughness(frame, s.normal, roughness), 0.045, 1.0);
    s.diffuse = base.rgb * (1.0 - metallic);
    s.f0 = lerp(float3(surface.reflectance), base.rgb, metallic);

    // A surface turned from the sun takes none of its light, shadow or not, so its shadow is not looked for. A
    // custom shader's own light may light it through, as the back of a wave is, so its shape's facing decides.
    float3 facing = customLit ? normalize(input.normal) : s.normal;
    bool facingSun = dot(facing, -frame->sunDirection) > 0.0;
    float shadow = facingSun ? min(sunShadow(frame, input.worldPosition, s.normal), contactShadow(frame, input.position.xy)) : 0.0;
    bool atmosphere = frame->lighting.sky == SKY_ATMOSPHERE;
    // Through the atmosphere, the sun reddens as it sets.
    float3 sunLight = atmosphere ? sunThroughAtmosphere(frame) : frame->sunColor;
    float3 sunThrough = sunLight * sunCookieAt(frame, input.worldPosition);
    sunThrough *= glassSun(frame, input.worldPosition, s.normal);
    // Under water, the sun comes down through it in caustics, and the sky's light is dimmed.
    float3 skyUnder;
    WaterOver water = waterOverAt(frame, input.worldPosition);
    sunThrough *= underwaterSunUnder(frame, input.worldPosition, water, skyUnder);
    float3 color = shade(s, -frame->sunDirection, sunThrough) * shadow;
    if (customLit)
    {
        LightInput sun = { -frame->sunDirection, sunThrough * shadow, s.view };
        color = customLight(input.material, sun, surface);
    }
    if (!customLit && (material.flags & MATERIAL_FOLIAGE) != 0)
    {
        // A leaf is thin, so its shaded side takes the light of its sunlit
        // one: the shadow there, a little way towards the sun, rather than
        // the leaf's own shadow on itself.
        static const float LEAF_DEPTH = 0.15;
        // Light through a leaf is soft already, so its shadow need not be: the hard one costs one look, not 28.
        float leafShadow = sunShadowHard(frame, input.worldPosition - frame->sunDirection * LEAF_DEPTH, -frame->sunDirection);
        color += foliageLight(s, -frame->sunDirection, sunThrough, leafShadow);
    }
    color += shadowTint(frame, s, -frame->sunDirection, sunThrough, shadow);
    float viewDepth = dot(input.worldPosition - frame->cameraPosition, frame->cameraForward);
    uint cluster = clusterAt(frame, input.position.xy, viewDepth);
    uint clusterBase = cluster * (frame->clusterMaxLights + 1);
    uint clusterLights = buffers()->clusters[clusterBase];
    for (uint i = 0; i < clusterLights; i++)
    {
        uint light = buffers()->clusters[clusterBase + 1 + i];
        // Dimmed by the water between, where either is under it.
        float3 wet = lampThroughWater(frame, buffers()->lights[light].position, input.worldPosition, water);
        if (customLit)
        {
            LightInput lamp = localLightInput(buffers()->lights[light], input.worldPosition, s.normal);
            lamp.view = s.view;
            lamp.color *= wet;
            color += customLight(input.material, lamp, surface);
            continue;
        }
        color += shadeLocalLight(s, buffers()->lights[light], input.worldPosition) * wet;
    }

    // The bounced light where the mesh has the surface, as the rays that gather it see it: moved by its vertex
    // function, as a wave's trough below the flat sheet the rays know, it read the probes under the sheet, in the dark.
    float3 lightAt = input.worldPosition - input.moved;
    float3 ambient = ambientLight(frame, lightAt, s, sunLight, atmosphere) * skyUnder;
    float nv = saturate(dot(s.normal, s.view));
    float3 ambientFresnel = s.f0 + (max(float3(1.0 - s.roughness), s.f0) - s.f0) * pow(1.0 - nv, 5.0);
    Occlusion occlusion = ambientOcclusion(frame, input.position.xy, nv, s.roughness);
    float found;
    float3 reflected = environmentReflection(frame, input.position.xy, lightAt, s.normal, reflect(-s.view, s.normal), s.roughness, ambient, atmosphere, found);
    // A reflection that was found already stops where something is in its way, so ambient occlusion only darkens
    // the share that came from the surroundings. Over all of it, its screen-space guesses about what blocks a
    // curved mirror lay grey smudges across a chrome ball that moved with the camera.
    float specularOcclusion = lerp(occlusion.specular, 1.0, found);
    color += ambient * s.diffuse * occlusion.diffuse + reflected * ambientFresnel * (1.0 - s.roughness * 0.7) * specularOcclusion;

    color += emission;
    if (atmosphere)
    {
        float2 screenUv = input.position.xy / float2(frame->viewport);
        float4 haze = aerialPerspectiveAt(frame, screenUv, length(input.worldPosition - frame->cameraPosition));
        color = color * haze.a + haze.rgb * skyLook(frame);
    }

    if (frame->debugView == DEBUG_VIEW_LOD)
    {
        color = debugColor(input.lod) * (0.35 + 0.65 * shadow * saturate(dot(s.normal, -frame->sunDirection)));
    }
    else if (frame->debugView == DEBUG_VIEW_MESHLETS)
    {
        color = keyColor(input.cluster) * (0.35 + 0.65 * shadow * saturate(dot(s.normal, -frame->sunDirection)));
    }
    else if (frame->debugView == DEBUG_VIEW_CASCADES)
    {
        color *= lerp(float3(1.0), debugColor(cascadeAt(frame, input.worldPosition)), 0.5);
    }
    else if (frame->debugView == DEBUG_VIEW_LIGHT_COMPLEXITY)
    {
        float heat = saturate(float(clusterLights) / 32.0);
        color = lerp(float3(0.05, 0.08, 0.2), lerp(float3(0.1, 0.9, 0.4), float3(1.0, 0.15, 0.1), heat), min(heat * 4.0, 1.0));
    }
    else if (frame->debugView == DEBUG_VIEW_NORMALS)
    {
        color = s.normal * 0.5 + 0.5;
    }
    else if (frame->debugView == DEBUG_VIEW_UNLIT)
    {
        color = base.rgb + emission;
    }
    else if (frame->debugView == DEBUG_VIEW_AMBIENT_OCCLUSION)
    {
        color = occlusion.diffuse;
    }
    else if (frame->debugView == DEBUG_VIEW_GLOBAL_ILLUMINATION)
    {
        color = ambient;
    }
    else if (frame->debugView == DEBUG_VIEW_WIREFRAME)
    {
        // Drawn as lines by the wireframe pipeline, in one colour.
        color = float3(0.78, 0.80, 0.86);
    }
    FragmentOut o;
    o.color = float4(color, 1.0);
    o.velocity = motion(input.currentClip, input.previousClip);
    return o;
}
