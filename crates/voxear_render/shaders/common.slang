// Types and bindings shared by every shader. Each struct mirrors one in
// crates/voxear_render/src/gpu.rs under scalar layout; change both together.
// Matrices are stored as four columns and applied with `transform`, so there
// is never any doubt about row or column order.

#pragma once

[[vk::binding(0, 0)]] Texture2D textures[];
[[vk::binding(1, 0)]] SamplerState samplers[];

static const uint SAMPLER_LINEAR_REPEAT = 0;
static const uint SAMPLER_LINEAR_CLAMP = 1;
static const uint SAMPLER_NEAREST_CLAMP = 2;
static const uint SAMPLER_MIN_REDUCTION = 3;

static const float PI = 3.14159265359;

struct Mat4
{
    float4 c0;
    float4 c1;
    float4 c2;
    float4 c3;
};

float4 transform(Mat4 m, float4 v)
{
    return m.c0 * v.x + m.c1 * v.y + m.c2 * v.z + m.c3 * v.w;
}

struct Vertex
{
    float3 position;
    float3 normal;
    float2 uv;
    float4 tangent;
};

struct Mesh
{
    uint vertexOffset;
    uint indexOffset;
    uint indexCount;
    uint pad;
    float3 center;
    float radius;
};

static const uint MATERIAL_NORMAL_MAP = 1;

struct Material
{
    float4 baseColor;
    float3 emissive;
    float metallic;
    float roughness;
    uint baseColorTexture;
    uint metallicRoughnessTexture;
    uint normalTexture;
    uint emissiveTexture;
    uint flags;
    float normalScale;
    float alphaCutoff;
};

struct Instance
{
    Mat4 model;
    float4 normal0;
    float4 normal1;
    float4 normal2;
    uint mesh;
    uint material;
    uint2 pad;
};

static const uint LIGHT_POINT = 0;
static const uint LIGHT_SPOT = 1;

struct Light
{
    float3 position;
    float range;
    float3 color;
    uint kind;
    float3 direction;
    float cosOuter;
    float cosInner;
    float3 pad;
};

struct Frame
{
    Mat4 viewProj;
    float4 planes[6];
    float3 cameraPosition;
    float time;
    float3 sunDirection;
    uint lightCount;
    float3 sunColor;
    float exposure;
    float3 skyZenith;
    float pad0;
    float3 skyHorizon;
    float pad1;
    float3 groundColor;
    float pad2;
    uint2 viewport;
    uint instanceCount;
    uint pad3;
    Mat4 invViewProj;
};

struct DrawCommand
{
    uint indexCount;
    uint instanceCount;
    uint firstIndex;
    int vertexOffset;
    uint firstInstance;
};

struct Push
{
    Frame* frame;
    Instance* instances;
    Mesh* meshes;
    Vertex* vertices;
    Material* materials;
    Light* lights;
    DrawCommand* draws;
    uint* drawCount;
    uint* drawInstances;
    uint texture;
    uint maxDraws;
};

[[vk::push_constant]] ConstantBuffer<Push> push;

float4 sampleTexture(uint index, float2 uv)
{
    return textures[NonUniformResourceIndex(index)].Sample(samplers[SAMPLER_LINEAR_REPEAT], uv);
}

// A triangle that covers the screen, from three vertex ids and no buffers.
float2 fullscreenNdc(uint vertexId)
{
    float2 uv = float2((vertexId << 1) & 2, vertexId & 2);
    return uv * 2.0 - 1.0;
}
