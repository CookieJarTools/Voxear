// Previews: an object drawn on its own into a small image, as the editor's
// Project panel, reference fields and Inspector show it, and as games can ask
// for. See src/previews.rs.
//
// The object is lit as in a photo studio: a key light (the frame's sun), a
// fill from the camera's side so nothing goes black, and the sky's
// hemisphere ambient. It is drawn at a multiple of its size, then the resolve
// averages each pixel's block, tonemaps it and lays it over the background,
// into the preview's cell of the atlas. Each draw finds its instance in
// push.list.

#include "common.slang"

// How strong the fill light is against the key.
static const float FILL = 0.35;

struct PreviewOut
{
    float4 position : SV_Position;
    float3 world : POSITION;
    float3 normal : NORMAL;
    float2 uv : TEXCOORD0;
    nointerpolation uint material : MATERIAL;
};

[shader("vertex")]
PreviewOut previewVertex(uint vertexId : SV_VertexID)
{
    Instance instance = buffers()->instances[push.list];
    Mesh mesh = buffers()->meshes[instance.mesh];
    Vertex v = buffers()->vertices[mesh.vertexOffset + vertexId];
    float4 world = transform(instance.model, float4(v.position, 1.0));
    PreviewOut o;
    o.position = transform(push.frame->viewProj, world);
    o.world = world.xyz;
    o.normal = instance.normal0.xyz * v.normal.x + instance.normal1.xyz * v.normal.y + instance.normal2.xyz * v.normal.z;
    o.uv = v.uv;
    o.material = instance.material;
    return o;
}

[shader("fragment")]
float4 previewFragment(PreviewOut input) : SV_Target
{
    Frame* frame = push.frame;
    Material material = buffers()->materials[input.material];
    float4 base = material.baseColor * sampleTexture(material.baseColorTexture, input.uv);
    if (material.alphaCutoff > 0.0 && base.a < material.alphaCutoff)
    {
        discard;
    }
    float4 metalRough = sampleTexture(material.metallicRoughnessTexture, input.uv);
    float metallic = material.metallic * metalRough.b;

    Surface s;
    s.view = normalize(frame->cameraPosition - input.world);
    // Seen from behind, as a plane can be, a face is lit as its front.
    s.normal = normalize(input.normal);
    s.normal = dot(s.normal, s.view) < 0.0 ? -s.normal : s.normal;
    s.roughness = clamp(material.roughness * metalRough.g, 0.045, 1.0);
    s.diffuse = base.rgb * (1.0 - metallic);
    s.f0 = lerp(float3(0.04), base.rgb, metallic);

    float3 color = shade(s, -frame->sunDirection, frame->sunColor);
    float3 fill = normalize(s.view + float3(0.0, 0.25, 0.0) - frame->sunDirection * 0.3);
    color += shade(s, fill, frame->sunColor * FILL);

    float up = s.normal.y * 0.5 + 0.5;
    float3 ambient = lerp(frame->groundColor, lerp(frame->skyHorizon, frame->skyZenith, 0.5), up) * 0.35 * frame->ambient;
    float nv = saturate(dot(s.normal, s.view));
    float3 ambientFresnel = s.f0 + (max(float3(1.0 - s.roughness), s.f0) - s.f0) * pow(1.0 - nv, 5.0);
    color += ambient * (s.diffuse + ambientFresnel * (1.0 - s.roughness * 0.7));
    color += material.emissive * sampleTexture(material.emissiveTexture, input.uv).rgb;
    return float4(color, 1.0);
}

struct ImpostorPicture
{
    float4 color : SV_Target0;
    float4 normal : SV_Target1;
};

// A plant pictured for its impostor (src/impostors.rs): its colour, unlit,
// with its cut-out, and its normal in its own space, as 0 to 1, facing the
// picture's camera where a leaf is seen from behind.
[shader("fragment")]
ImpostorPicture impostorBakeFragment(PreviewOut input)
{
    Frame* frame = push.frame;
    Material material = buffers()->materials[input.material];
    float4 base = material.baseColor * sampleTexture(material.baseColorTexture, input.uv);
    if (material.alphaCutoff > 0.0 && base.a < material.alphaCutoff)
    {
        discard;
    }
    float3 n = normalize(input.normal);
    n = dot(n, -frame->cameraForward) < 0.0 ? -n : n;
    ImpostorPicture o;
    o.color = float4(base.rgb, 1.0);
    o.normal = float4(n * 0.5 + 0.5, 1.0);
    return o;
}

[shader("vertex")]
float4 resolveVertex(uint vertexId : SV_VertexID) : SV_Position
{
    return float4(fullscreenNdc(vertexId), 0.0, 1.0);
}

// The ACES filmic curve, as fitted by Krzysztof Narkowicz.
float3 filmic(float3 x)
{
    return saturate((x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14));
}

// One pixel of the cell: the block of drawn pixels it covers, each
// tonemapped, then averaged, so edges are smooth; over the background, as
// premultiplied colour. push.size is the cell's corner in the atlas,
// push.level the block's size, push.maxDraws the drawn image's size,
// push.textures the background (linear, straight alpha) and push.output the
// exposure, both as float bits.
[shader("fragment")]
float4 resolveFragment(float4 position : SV_Position) : SV_Target
{
    uint2 local = uint2(position.xy) - push.size;
    uint scale = max(push.level, 1);
    float exposure = asfloat(push.output);
    float4 sum = 0.0;
    for (uint y = 0; y < scale; y++)
    {
        for (uint x = 0; x < scale; x++)
        {
            float2 uv = (float2(local * scale + uint2(x, y)) + 0.5) / float(push.maxDraws);
            float4 drawn = textures[NonUniformResourceIndex(push.texture)].SampleLevel(samplers[SAMPLER_NEAREST_CLAMP], uv, 0.0);
            sum += float4(filmic(drawn.rgb * exposure) * drawn.a, drawn.a);
        }
    }
    sum /= float(scale * scale);
    float4 background = asfloat(push.textures);
    float behind = background.a * (1.0 - sum.a);
    return float4(sum.rgb + background.rgb * behind, sum.a + behind);
}
