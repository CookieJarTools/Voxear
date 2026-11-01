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
static const uint STAT_FADE_DRAWS = STAT_SHADOW_DRAWS + 1;
static const uint STAT_CLUSTER_MAX_LIGHTS = STAT_FADE_DRAWS + 1;
static const uint STAT_CLUSTER_LIGHT_LINKS = STAT_CLUSTER_MAX_LIGHTS + 1;

static const uint DEBUG_VIEW_NONE = 0;
static const uint DEBUG_VIEW_LOD = 1;
static const uint DEBUG_VIEW_CASCADES = 2;
static const uint DEBUG_VIEW_LIGHT_COMPLEXITY = 3;
static const uint DEBUG_VIEW_NORMALS = 4;
static const uint DEBUG_VIEW_UNLIT = 5;
static const uint DEBUG_VIEW_WIREFRAME = 6;
static const uint DEBUG_VIEW_LIT_WIREFRAME = 7;

// A draw's instance index, with its LOD and fade role packed in the top byte.
static const uint DRAW_INSTANCE_MASK = 0xFFFFFF;
static const uint DRAW_LOD_SHIFT = 24;
static const uint DRAW_FADING_IN = 1u << 28;
static const uint DRAW_FADING_OUT = 1u << 29;

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
    Mat4 prevModel;
    float4 normal0;
    float4 normal1;
    float4 normal2;
    uint mesh;
    uint material;
    uint flags;
    uint pad;
};

// An instance whose place in the list another Obj held last frame.
static const uint INSTANCE_NEW = 1;

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

// Post-processing settings; mirrors GpuPost in src/gpu.rs.
struct Post
{
    float bloomIntensity;
    float sharpness;
    float taaHistory;
    float chromaticAberration;
    float exposureCompensation;
    float exposureMin;
    float exposureMax;
    uint autoExposure;
    float exposureSpeedUp;
    float exposureSpeedDown;
    uint tonemapper;
    uint fxaa;
    float tonemapContrast;
    float tonemapSaturation;
    float temperature;
    float tint;
    float gradingContrast;
    float gradingSaturation;
    float vignette;
    float vignetteSmoothness;
    float3 lift;
    float vignetteRoundness;
    float3 gamma;
    float grain;
    float3 gain;
    float grainSize;
    float3 fogColor;
    float fogDensity;
    float fogHeightFalloff;
    float fogBaseHeight;
    float fogSunGlow;
    float bloomScatter;
    float dofFocus;
    float dofAperture;
    float dofMaxBlur;
    float motionBlur;
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
    Mat4 view;
    uint3 clusterGrid;
    uint clusterMaxLights;
    float clusterScale;
    float clusterBias;
    float2 tileSize;
    float deltaTime;
    float lodFadeSpeed;
    float ambient;
    float pad4;
    Mat4 unjitteredViewProj;
    Mat4 prevViewProj;
    float2 jitter;
    uint taaReset;
    uint frameIndex;
    Post post;
};

struct DrawCommand
{
    uint indexCount;
    uint instanceCount;
    uint firstIndex;
    int vertexOffset;
    uint firstInstance;
};

// Device addresses of every buffer, written once per frame->
// A rectangle or glyph of the UI. Mirrors `Quad` in voxear_ui.
struct UiQuad
{
    float4 rect;
    float4 uv;
    float4 color;
    float4 borderColor;
    float4 clip;
    float radius;
    float border;
    uint kind;
    uint flags;
};

// A line of debug draw. Mirrors `GpuDebugLine` in src/gpu.rs.
struct DebugLine
{
    float3 from;
    uint flags;
    float3 to;
    float pad;
    float4 color;
};

struct Buffers
{
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
    uint* clusters;
    uint* lodState;
    float* exposure;
    UiQuad* uiQuads;
    DebugLine* debugLines;
};

struct Push
{
    Frame* frame;
    Buffers* buffers;
    uint texture;
    uint maxDraws;
    uint list;
    uint output;
    uint2 size;
    uint level;
    uint pad;
    uint4 textures;
};

[[vk::push_constant]] ConstantBuffer<Push> push;

Buffers* buffers()
{
    return push.buffers;
}

uint drawInstance(uint drawId, out uint lod, out uint fade)
{
    uint packed = buffers()->drawInstances[push.list * push.maxDraws + drawId];
    lod = (packed >> DRAW_LOD_SHIFT) & 0xF;
    fade = packed & (DRAW_FADING_IN | DRAW_FADING_OUT);
    return packed & DRAW_INSTANCE_MASK;
}


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

// Physically based shading (glTF metal/rough), shared by the forward pass and
// previews.
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
