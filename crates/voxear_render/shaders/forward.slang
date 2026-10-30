// Forward PBR (glTF metal/rough) for opaque geometry. Vertices are pulled
// from the shared vertex buffer; each indirect draw finds its instance
// through the draw index the culling pass recorded.

#include "common.slang"

struct VertexOut
{
    float4 position : SV_Position;
    float3 worldPosition : POSITION;
    float3 normal : NORMAL;
    float4 tangent : TANGENT;
    float2 uv : TEXCOORD0;
    nointerpolation uint material : MATERIAL;
};

[shader("vertex")]
VertexOut vertexMain(uint vertexId : SV_VertexID, uint drawId : SV_DrawIndex)
{
    Instance instance = push.instances[push.drawInstances[drawId]];
    Mesh mesh = push.meshes[instance.mesh];
    Vertex v = push.vertices[mesh.vertexOffset + vertexId];
    float4 world = transform(instance.model, float4(v.position, 1.0));

    VertexOut o;
    o.position = transform(push.frame[0].viewProj, world);
    o.worldPosition = world.xyz;
    o.normal = instance.normal0.xyz * v.normal.x + instance.normal1.xyz * v.normal.y + instance.normal2.xyz * v.normal.z;
    o.tangent = float4(transform(instance.model, float4(v.tangent.xyz, 0.0)).xyz, v.tangent.w);
    o.uv = v.uv;
    o.material = instance.material;
    return o;
}

float distributionGgx(float nh, float a)
{
    float a2 = a * a;
    float d = nh * nh * (a2 - 1.0) + 1.0;
    return a2 / (PI * d * d);
}

float visibilitySmithGgx(float nv, float nl, float a)
{
    float a2 = a * a;
    float gv = nl * sqrt(nv * nv * (1.0 - a2) + a2);
    float gl = nv * sqrt(nl * nl * (1.0 - a2) + a2);
    return 0.5 / max(gv + gl, 1e-5);
}

float3 fresnelSchlick(float3 f0, float vh)
{
    return f0 + (1.0 - f0) * pow(1.0 - vh, 5.0);
}

struct Surface
{
    float3 normal;
    float3 view;
    float3 diffuse;
    float3 f0;
    float roughness;
};

float3 shade(Surface s, float3 toLight, float3 radiance)
{
    float nl = dot(s.normal, toLight);
    if (nl <= 0.0)
    {
        return 0.0;
    }
    float3 h = normalize(s.view + toLight);
    float nv = max(dot(s.normal, s.view), 1e-4);
    float nh = saturate(dot(s.normal, h));
    float vh = saturate(dot(s.view, h));
    float a = s.roughness * s.roughness;
    float3 f = fresnelSchlick(s.f0, vh);
    float3 specular = distributionGgx(nh, a) * visibilitySmithGgx(nv, nl, a) * f;
    float3 diffuse = s.diffuse * (1.0 - f) / PI;
    return (diffuse + specular) * radiance * nl;
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
    float3 m = sampleTexture(material.normalTexture, input.uv).xyz * 2.0 - 1.0;
    m.xy *= material.normalScale;
    return normalize(t * m.x + b * m.y + n * m.z);
}

[shader("fragment")]
float4 fragmentMain(VertexOut input) : SV_Target
{
    Frame frame = push.frame[0];
    Material material = push.materials[input.material];

    float4 base = material.baseColor * sampleTexture(material.baseColorTexture, input.uv);
    if (material.alphaCutoff > 0.0 && base.a < material.alphaCutoff)
    {
        discard;
    }
    float4 metalRough = sampleTexture(material.metallicRoughnessTexture, input.uv);
    float metallic = material.metallic * metalRough.b;

    Surface s;
    s.normal = surfaceNormal(input, material);
    s.view = normalize(frame.cameraPosition - input.worldPosition);
    s.roughness = clamp(material.roughness * metalRough.g, 0.045, 1.0);
    s.diffuse = base.rgb * (1.0 - metallic);
    s.f0 = lerp(float3(0.04), base.rgb, metallic);

    float3 color = shade(s, -frame.sunDirection, frame.sunColor);
    for (uint i = 0; i < frame.lightCount; i++)
    {
        color += shadeLocalLight(s, push.lights[i], input.worldPosition);
    }

    // Hemisphere ambient until real GI arrives.
    float up = s.normal.y * 0.5 + 0.5;
    float3 ambient = lerp(frame.groundColor, lerp(frame.skyHorizon, frame.skyZenith, 0.5), up) * 0.35;
    float nv = saturate(dot(s.normal, s.view));
    float3 ambientFresnel = s.f0 + (max(float3(1.0 - s.roughness), s.f0) - s.f0) * pow(1.0 - nv, 5.0);
    color += ambient * (s.diffuse + ambientFresnel * (1.0 - s.roughness * 0.7));

    color += material.emissive * sampleTexture(material.emissiveTexture, input.uv).rgb;
    return float4(color, 1.0);
}
