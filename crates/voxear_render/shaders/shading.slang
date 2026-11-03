// Forward PBR (glTF metal/rough) for opaque geometry: what the vertex
// shader (forward.slang) and the mesh shaders (meshlets.slang) both make of
// a vertex, and the fragment shaders that shade it. Vertices are pulled from
// the shared vertex buffer.

#pragma once

#include "common.slang"

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
    Vertex v = buffers()->vertices[buffers()->meshes[instance.mesh].vertexOffset + vertexIndex];
    float4 world = transform(instance.model, float4(v.position, 1.0));

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
    o.previousClip = transform(push.frame->prevViewProj, transform(instance.prevModel, float4(v.position, 1.0)));
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

// The incoming LOD covers `fade` of the pixels and the outgoing one the rest.
void ditherLodFade(VertexOut input)
{
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

float3 shadeLocalLight(Surface s, Light light, float3 position)
{
    float3 toLight = light.position - position;
    float distanceSquared = max(dot(toLight, toLight), 1e-4);
    toLight *= rsqrt(distanceSquared);
    // Inverse square, smoothly windowed to zero at the range.
    float ratio = distanceSquared / max(light.range * light.range, 1e-4);
    float window = saturate(1.0 - ratio * ratio);
    float attenuation = window * window / distanceSquared;
    if (light.kind == LIGHT_SPOT)
    {
        attenuation *= smoothstep(light.cosOuter, light.cosInner, dot(-toLight, light.direction));
    }
    return shade(s, toLight, light.color * attenuation);
}

float3 surfaceNormal(VertexOut input, Material material)
{
    float3 n = normalize(input.normal);
    if ((material.flags & MATERIAL_NORMAL_MAP) == 0)
    {
        return n;
    }
    float3 t = normalize(input.tangent.xyz - n * dot(n, input.tangent.xyz));
    float3 b = cross(n, t) * input.tangent.w;
    // Only X and Y are read: Z follows from them, which lets normal maps ship as two-channel BC5.
    float2 xy = sampleTexture(material.normalTexture, input.uv).xy * 2.0 - 1.0;
    float3 m = float3(xy * material.normalScale, sqrt(saturate(1.0 - dot(xy, xy))));
    return normalize(t * m.x + b * m.y + n * m.z);
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

// 3x3 taps of hardware 2x2 PCF across one cascade's tile of the atlas.
float sunShadow(Frame* frame, float3 position, float3 normal)
{
    uint cascade = cascadeAt(frame, position);
    if (frame->shadowTexture == NO_TEXTURE || cascade >= CASCADES)
    {
        return 1.0;
    }
    float3 offsetPosition = position + normal * frame->cascadeTexel[cascade] * 1.5;
    float4 clip = transform(frame->cascadeViewProj[cascade], float4(offsetPosition, 1.0));
    float2 uv = clip.xy * 0.5 + 0.5;
    if (any(uv < 0.0) || any(uv > 1.0) || clip.z > 1.0)
    {
        return 1.0;
    }

    float texel = 0.5 / float(push.size.x);
    float2 tile = CASCADE_TILES[cascade];
    float2 low = tile + texel * 1.5;
    float2 high = tile + 0.5 - texel * 1.5;
    float lit = 0.0;
    for (int y = -1; y <= 1; y++)
    {
        for (int x = -1; x <= 1; x++)
        {
            float2 atlasUv = clamp(tile + uv * 0.5 + float2(x, y) * texel * 2.0, low, high);
            lit += textures[frame->shadowTexture].SampleCmpLevelZero(comparisonSamplers[COMPARISON_SAMPLER_SHADOW], atlasUv, clip.z);
        }
    }
    return lit / 9.0;
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

    Surface s;
    s.normal = surfaceNormal(input, material);
    s.view = normalize(frame->cameraPosition - input.worldPosition);
    s.roughness = clamp(material.roughness * metalRough.g, 0.045, 1.0);
    s.diffuse = base.rgb * (1.0 - metallic);
    s.f0 = lerp(float3(0.04), base.rgb, metallic);

    float shadow = sunShadow(frame, input.worldPosition, s.normal);
    float3 color = shade(s, -frame->sunDirection, frame->sunColor) * shadow;
    float viewDepth = dot(input.worldPosition - frame->cameraPosition, frame->cameraForward);
    uint cluster = clusterAt(frame, input.position.xy, viewDepth);
    uint clusterBase = cluster * (frame->clusterMaxLights + 1);
    uint clusterLights = buffers()->clusters[clusterBase];
    for (uint i = 0; i < clusterLights; i++)
    {
        uint light = buffers()->clusters[clusterBase + 1 + i];
        color += shadeLocalLight(s, buffers()->lights[light], input.worldPosition);
    }

    // Hemisphere ambient until real GI arrives.
    float up = s.normal.y * 0.5 + 0.5;
    float3 ambient = lerp(frame->groundColor, lerp(frame->skyHorizon, frame->skyZenith, 0.5), up) * 0.35 * frame->ambient;
    float nv = saturate(dot(s.normal, s.view));
    float3 ambientFresnel = s.f0 + (max(float3(1.0 - s.roughness), s.f0) - s.f0) * pow(1.0 - nv, 5.0);
    color += ambient * (s.diffuse + ambientFresnel * (1.0 - s.roughness * 0.7));

    color += material.emissive * sampleTexture(material.emissiveTexture, input.uv).rgb;

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
