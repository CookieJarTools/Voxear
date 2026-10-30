// Types and bindings shared by every shader. Each struct mirrors one in
// crates/voxear_render/src/gpu.rs under scalar layout; change both together.
// Matrices are stored as four columns and applied with `transform`, so there
// is never any doubt about row or column order.

#pragma once

[[vk::binding(0, 0)]] Texture2D textures[];
[[vk::binding(1, 0)]] SamplerState samplers[];
[[vk::binding(2, 0)]] [[vk::image_format("r32f")]] RWTexture2D<float> storageImagesR32f[];
[[vk::binding(3, 0)]] SamplerComparisonState comparisonSamplers[];

static const uint SAMPLER_LINEAR_REPEAT = 0;
static const uint SAMPLER_LINEAR_CLAMP = 1;
static const uint SAMPLER_NEAREST_CLAMP = 2;
static const uint SAMPLER_MIN_REDUCTION = 3;
static const uint COMPARISON_SAMPLER_SHADOW = 0;

static const uint NO_TEXTURE = 0xFFFFFFFF;
static const uint MAX_LODS = 6;
static const uint CASCADES = 4;

static const uint LIST_EARLY = 0;
static const uint LIST_LATE = 1;
static const uint LIST_SHADOW = 2;

static const uint STAT_TESTED = 0;
static const uint STAT_FRUSTUM_CULLED = 1;
static const uint STAT_OCCLUSION_CULLED = 2;
static const uint STAT_DRAWN_EARLY = 3;
static const uint STAT_DRAWN_LATE = 4;
static const uint STAT_LOD = 5;
static const uint STAT_SHADOW_DRAWS = STAT_LOD + MAX_LODS;

static const uint DEBUG_VIEW_NONE = 0;
static const uint DEBUG_VIEW_LOD = 1;
static const uint DEBUG_VIEW_CASCADES = 2;

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

struct MeshLod
{
    uint indexOffset;
    uint indexCount;
    float error;
    uint pad;
};

struct Mesh
{
    uint vertexOffset;
    uint lodCount;
    uint2 pad;
    float3 center;
    float radius;
    MeshLod lods[MAX_LODS];
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
    Mat4 invViewProj;
    float4 planes[6];
    float3 cameraPosition;
    float time;
    float3 cameraForward;
    float near;
    float3 sunDirection;
    uint lightCount;
    float3 sunColor;
    float exposure;
    float3 skyZenith;
    float lodScale;
    float3 skyHorizon;
    float lodThreshold;
    float3 groundColor;
    uint debugView;
    uint2 viewport;
    uint instanceCount;
    uint hizTexture;
    uint2 hizSize;
    uint hizMips;
    uint shadowTexture;
    Mat4 cascadeViewProj[CASCADES];
    float4 cascadePlanes[CASCADES][6];
    float cascadeFar[CASCADES];
    float cascadeTexel[CASCADES];
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
    uint* visibility;
    uint* stats;
    uint texture;
    uint maxDraws;
    uint list;
    uint output;
    uint2 size;
    uint level;
    uint pad;
};

// Culling packs the chosen LOD into the top byte of each draw's instance index.
uint drawInstance(uint drawId, out uint lod)
{
    uint packed = push.drawInstances[push.list * push.maxDraws + drawId];
    lod = packed >> 24;
    return packed & 0xFFFFFF;
}

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
