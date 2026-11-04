// Forward PBR (glTF metal/rough) for opaque geometry: what the vertex
// shader (forward.slang) and the mesh shaders (meshlets.slang) both make of
// a vertex, and the fragment shaders that shade it. Vertices are pulled from
// the shared vertex buffer.

#pragma once

#include "atmosphere.slang"
#include "common.slang"
#include "ddgi_sample.slang"
#include "glow.slang"
#include "lights.slang"
#include "occlusion.slang"
#include "seasons.slang"
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
    Vertex v = buffers()->vertices[mesh.vertexOffset + vertexIndex];
    float4 world = transform(instance.model, float4(v.position, 1.0));
    float4 previous = transform(instance.prevModel, float4(v.position, 1.0));
    // Plants move in the wind, now and as they were a frame ago, for their motion.
    Material material = buffers()->materials[instance.material];
    float3 normal = normalize(instance.normal0.xyz * v.normal.x + instance.normal1.xyz * v.normal.y + instance.normal2.xyz * v.normal.z);
    float height = plantHeight(instance, mesh);
    Frame* frame = push.frame;
    world.xyz += windOffset(frame, material, instance.model.c3.xyz, height, world.xyz, normal, frame->time);
    float earlier = frame->time - frame->deltaTime;
    previous.xyz += windOffset(frame, material, instance.prevModel.c3.xyz, height, previous.xyz, normal, earlier);

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

// Ordered 4x4 dither, for fading between LODs without blending.
float bayer4(uint2 pixel)
{
    static const float MATRIX[16] = { 0, 8, 2, 10, 12, 4, 14, 6, 3, 11, 1, 9, 15, 7, 13, 5 };
    return (MATRIX[(pixel.y & 3) * 4 + (pixel.x & 3)] + 0.5) / 16.0;
}

// The incoming LOD covers `fade` of the pixels and the outgoing one the rest;
// and an instance that fades out with distance, as grass does, covers less
// and less of its pixels towards where it is gone.
void ditherLodFade(VertexOut input)
{
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
    bool incoming = bayer4(uint2(input.position.xy)) < fade;
    if (incoming != ((input.fade & DRAW_FADING_IN) != 0))
    {
        discard;
    }
}

// `v` at length 1, or `otherwise` where it has none to speak of (or is not
// a number), which normalize would make NaN, and bloom then spread over
// the whole picture.
float3 normalizeOr(float3 v, float3 otherwise)
{
    float lengthSquared = dot(v, v);
    return lengthSquared > 1e-12 ? v * rsqrt(lengthSquared) : otherwise;
}

float3 surfaceNormal(VertexOut input, Material material)
{
    // Facing the camera where a mesh's own normal is missing, as where two opposite normals meet across a face.
    float3 n = normalizeOr(input.normal, normalize(push.frame->cameraPosition - input.worldPosition));
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
        // through a wall.
        sky = ddgiIrradiance(frame, position, direction, normal, sky);
    }
    return lerp(sky, ambient, blur);
}

// As surroundingsReflection, with screen-space reflections over it where
// they found something and the surface is smooth enough to show them.
float3 environmentReflection(Frame* frame, float2 pixel, float3 position, float3 normal, float3 direction, float roughness, float3 ambient, bool atmosphere)
{
    float3 reflected = surroundingsReflection(frame, position, normal, direction, roughness, ambient, atmosphere);
    if (frame->post.reflectionTexture == NO_TEXTURE)
    {
        return reflected;
    }
    float2 uv = pixel / float2(frame->viewport);
    float4 screen = textures[NonUniformResourceIndex(frame->post.reflectionTexture)].SampleLevel(samplers[SAMPLER_LINEAR_CLAMP], uv, 0.0);
    float smooth = saturate(1.0 - roughness / frame->post.reflectionMaxRoughness);
    return lerp(reflected, screen.rgb, screen.a * smooth);
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
    ShadowLookup lookup = shadowLookup(frame, position, normal);
    if (!lookup.found)
    {
        return 1.0;
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
    return lit / 9.0;
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
    ShadowLookup lookup = shadowLookup(frame, position, normal);
    if (!lookup.found)
    {
        return 1.0;
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
        return 1.0;
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
    return lit / float(PCSS_FILTER_TAPS);
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

[shader("fragment")]
FragmentOut fragmentMain(VertexOut input)
{
    Frame* frame = push.frame;
    ditherLodFade(input);
    Material material = buffers()->materials[input.material];

    float4 base = material.baseColor * sampleTexture(material.baseColorTexture, input.uv);
    if (material.alphaCutoff > 0.0 && base.a < material.alphaCutoff)
    {
        discard;
    }
    float4 metalRough = sampleTexture(material.metallicRoughnessTexture, input.uv);
    float metallic = material.metallic * metalRough.b;
    float roughness = material.roughness * metalRough.g;
    float3 origin = buffers()->instances[input.instance].model.c3.xyz;
    seasonalSurface(frame, material, origin, normalizeOr(input.normal, float3(0.0, 1.0, 0.0)), base.rgb, roughness);

    Surface s;
    s.normal = surfaceNormal(input, material);
    s.view = normalize(frame->cameraPosition - input.worldPosition);
    s.roughness = clamp(roughness, 0.045, 1.0);
    s.diffuse = base.rgb * (1.0 - metallic);
    s.f0 = lerp(float3(0.04), base.rgb, metallic);

    // A surface turned from the sun takes none of its light, shadow or not, so its shadow is not looked for.
    bool facingSun = dot(s.normal, -frame->sunDirection) > 0.0;
    float shadow = facingSun ? min(sunShadow(frame, input.worldPosition, s.normal), contactShadow(frame, input.position.xy)) : 0.0;
    bool atmosphere = frame->lighting.sky == SKY_ATMOSPHERE;
    // Through the atmosphere, the sun reddens as it sets.
    float3 sunLight = atmosphere ? sunThroughAtmosphere(frame) : frame->sunColor;
    float3 sunThrough = sunLight * sunCookieAt(frame, input.worldPosition);
    float3 color = shade(s, -frame->sunDirection, sunThrough) * shadow;
    if ((material.flags & MATERIAL_FOLIAGE) != 0)
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
        color += shadeLocalLight(s, buffers()->lights[light], input.worldPosition);
    }

    float3 ambient = ambientLight(frame, input.worldPosition, s, sunLight, atmosphere);
    float nv = saturate(dot(s.normal, s.view));
    float3 ambientFresnel = s.f0 + (max(float3(1.0 - s.roughness), s.f0) - s.f0) * pow(1.0 - nv, 5.0);
    Occlusion occlusion = ambientOcclusion(frame, input.position.xy, nv, s.roughness);
    float3 reflected = environmentReflection(frame, input.position.xy, input.worldPosition, s.normal, reflect(-s.view, s.normal), s.roughness, ambient, atmosphere);
    color += ambient * s.diffuse * occlusion.diffuse + reflected * ambientFresnel * (1.0 - s.roughness * 0.7) * occlusion.specular;

    color += material.emissive * sampleTexture(material.emissiveTexture, input.uv).rgb * glowAmount(frame, material, origin);
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
        color = base.rgb + material.emissive * sampleTexture(material.emissiveTexture, input.uv).rgb;
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
