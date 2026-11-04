// Types and bindings shared by every shader. Each struct mirrors one in
// crates/voxear_render/src/gpu.rs under scalar layout; change both together.
// Matrices are stored as four columns and applied with `transform`, so there
// is never any doubt about row or column order.

#pragma once

[[vk::binding(0, 0)]] Texture2D textures[];
[[vk::binding(1, 0)]] SamplerState samplers[];
[[vk::binding(2, 0)]] [[vk::image_format("r32f")]] RWTexture2D<float> storageImagesR32f[];
// The same storage images, for those of four half floats, as the GI probes'.
[[vk::binding(2, 0)]] [[vk::image_format("rgba16f")]] RWTexture2D<float4> storageImagesRgba16f[];
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
static const uint LIST_COUNT = LIST_SHADOW + CASCADES;

// Each instance list's draw count heads a dispatch's arguments (count, 1, 1).
// With meshlet culling the list's draws are not written: its meshlets are
// queued instead, and the first word is how many workgroups the meshlet pass
// runs for them, without the CPU reading it back, and the fourth how many
// were queued. Each meshlet list's count follows them all.
static const uint DRAW_COUNT_STRIDE = 4;

uint drawCountIndex(uint list)
{
    return list * DRAW_COUNT_STRIDE;
}

uint queuedCountIndex(uint list)
{
    return list * DRAW_COUNT_STRIDE + 3;
}

uint meshletCountIndex(uint list)
{
    return LIST_COUNT * DRAW_COUNT_STRIDE + list;
}

// Meshlet lists, one after another: the main view's two hold two draws'
// meshlets for each instance, since a fading instance draws two LODs; each
// shadow cascade's holds one. `capacity` is the most meshlets every
// instance's draw could turn into together, worked out on the CPU.
uint meshletListBase(uint list, uint capacity)
{
    return list < LIST_SHADOW ? list * 2 * capacity : (4 + list - LIST_SHADOW) * capacity;
}

uint meshletListCapacity(uint list, uint capacity)
{
    return list < LIST_SHADOW ? 2 * capacity : capacity;
}

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
static const uint STAT_MESHLETS_TESTED = STAT_CLUSTER_LIGHT_LINKS + 1;
static const uint STAT_MESHLETS_FRUSTUM_CULLED = STAT_MESHLETS_TESTED + 1;
static const uint STAT_MESHLETS_CONE_CULLED = STAT_MESHLETS_FRUSTUM_CULLED + 1;
static const uint STAT_MESHLETS_OCCLUSION_CULLED = STAT_MESHLETS_CONE_CULLED + 1;
static const uint STAT_MESHLETS_DRAWN = STAT_MESHLETS_OCCLUSION_CULLED + 1;
static const uint STAT_MESHLETS_SHADOW_DRAWN = STAT_MESHLETS_DRAWN + 1;

static const uint DEBUG_VIEW_NONE = 0;
static const uint DEBUG_VIEW_LOD = 1;
static const uint DEBUG_VIEW_CASCADES = 2;
static const uint DEBUG_VIEW_LIGHT_COMPLEXITY = 3;
static const uint DEBUG_VIEW_NORMALS = 4;
static const uint DEBUG_VIEW_UNLIT = 5;
static const uint DEBUG_VIEW_WIREFRAME = 6;
static const uint DEBUG_VIEW_LIT_WIREFRAME = 7;
static const uint DEBUG_VIEW_MESHLETS = 8;
static const uint DEBUG_VIEW_AMBIENT_OCCLUSION = 9;
static const uint DEBUG_VIEW_GLOBAL_ILLUMINATION = 10;

// Frame.meshletFlags: back-facing meshlets are culled; surviving meshlets
// side by side are drawn together; draws are culled meshlet by meshlet.
static const uint MESHLET_CONES = 1;
static const uint MESHLET_RUNS = 2;
static const uint MESHLET_CULLING = 4;

// Meshlets the meshlet pass, and a task shader, take at a time.
static const uint MESHLET_ROUND = 32;
// The most vertices and triangles a meshlet has (see src/meshlets.rs).
static const uint MESHLET_VERTICES = 64;
static const uint MESHLET_TRIANGLES = 124;

// Push.source: which lists a draw pass reads. With mesh shaders, DRAWS_AGAIN
// draws the meshlet lists a second time, as the lit wireframe's edges,
// without counting them in the stats again.
static const uint DRAWS_INSTANCES = 0;
static const uint DRAWS_MESHLETS = 1;
static const uint DRAWS_AGAIN = 2;

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
    // Its meshlets in the meshlet table, which hold its indices in turn.
    uint meshletOffset;
    uint meshletCount;
    uint pad;
};

struct Mesh
{
    uint vertexOffset;
    uint lodCount;
    // Where its meshlets' vertices and triangles start.
    uint meshletVertexOffset;
    uint meshletTriangleOffset;
    float3 center;
    float radius;
    MeshLod lods[MAX_LODS];
};

// Up to 124 of a LOD's triangles, in mesh space. Its normals lie within the
// cone around coneAxis whose half-angle has the cosine coneCutoff (1 when no
// viewpoint sees only their backs).
struct Meshlet
{
    float3 center;
    float radius;
    float3 coneAxis;
    float coneCutoff;
    // From its LOD's first index.
    uint firstIndex;
    // From its mesh's meshlet vertices and triangles.
    uint vertexOffset;
    uint triangleOffset;
    // Vertex count in the low byte, triangle count above.
    uint counts;
};

static const uint MATERIAL_NORMAL_MAP = 1;
// Lit as leaves are: wrapped, through the leaf, and with a rim (shading.slang's foliageLight).
static const uint MATERIAL_FOLIAGE = 2;
// Glass, drawn after the opaque scene over what lies behind it (glass.slang).
static const uint MATERIAL_GLASS = 4;

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
    // Glass's index of refraction.
    float ior;
    // How much plants drawn with it bend in the wind, and their leaves flutter (wind.slang).
    float windBend;
    float windFlutter;
    float pad;
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
    float lodBias;
    // With INSTANCE_LOD_SCREEN, the share of the screen's height each LOD shows down to.
    float4 lodScreen0;
    float4 lodScreen1;
};

// An instance whose place in the list another Obj held last frame.
static const uint INSTANCE_NEW = 1;
// Where an instance's forced LOD, plus one, sits in its flags.
static const uint INSTANCE_LOD_SHIFT = 8;
// An instance whose LODs switch by how much of the screen it covers, at its lodScreen.
static const uint INSTANCE_LOD_SCREEN = 2;

static const uint LIGHT_POINT = 0;
static const uint LIGHT_SPOT = 1;
static const uint LIGHT_RECTANGLE = 2;
static const uint LIGHT_DISC = 3;
static const uint LIGHT_TUBE = 4;

// A local light; mirrors GpuLight in src/gpu.rs. Area lights shine from one
// face along `direction`; a tube lies along `right`.
struct Light
{
    float3 position;
    float range;
    float3 color;
    uint kind;
    float3 direction;
    float cosOuter;
    float3 right;
    float cosInner;
    // Half width and height; a disc's radius twice; a tube's half length and radius.
    float2 size;
    uint cookie;
    uint profile;
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
    float padGamma;
    float3 gain;
    float padGain;
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
    // Ambient occlusion: the tint (linear) and strength, the radius in
    // metres, and the slices and steps GTAO searches; the image the opaque
    // passes read, NO_TEXTURE while it is off; what turns a pixel and its
    // view depth back into view space; and whether last frame's is stale.
    float3 aoTint;
    float aoStrength;
    float aoRadius;
    uint aoSlices;
    uint aoSteps;
    uint aoTexture;
    float2 aoUnproject;
    uint aoReset;
    uint aoPad;
    // Volumetric fog: Henyey-Greenstein's g, how far the volume reaches, the
    // integrated volume (NO_TEXTURE without it), and 1 when its history is stale.
    float fogAnisotropy;
    float fogVolumeFar;
    uint fogVolume;
    uint fogVolumeReset;
    // Contact shadows: their image (NO_TEXTURE without them), how far each
    // pixel looks towards the sun, in metres, how thick a thing in the depth
    // buffer counts as, and how many steps it takes.
    uint contactTexture;
    float contactLength;
    float contactThickness;
    uint contactSteps;
    // The colour grading lookup table (NO_TEXTURE without one), and how much
    // of it shows. Its size comes from the table itself.
    uint lutTexture;
    uint lutPad0;
    float lutContribution;
    uint lutPad;
    // Screen-space reflections: their image (NO_TEXTURE without them), how
    // far rays go, how thick things count as, in metres, the steps they take,
    // and the roughness past which surfaces show none.
    uint reflectionTexture;
    float reflectionDistance;
    float reflectionThickness;
    uint reflectionSteps;
    float reflectionMaxRoughness;
    // The part of auto exposure's histogram it averages, between these percentiles.
    float exposureLowPercent;
    float exposureHighPercent;
    uint exposurePad;
    // Bloom's colour, multiplying its glow.
    float3 bloomTint;
    // How dark auto exposure leaves dark scenes, 0 as bright as day, 1 as the eye sees them.
    float exposureNight;
};

static const uint SKY_GRADIENT = 0;
static const uint SKY_ATMOSPHERE = 1;

// A planet's atmosphere in kilometres; mirrors GpuAtmosphere in src/gpu.rs.
struct Atmosphere
{
    float3 rayleighScattering;
    float rayleighHeight;
    float3 mieScattering;
    float mieHeight;
    float3 mieExtinction;
    float mieG;
    float3 ozoneAbsorption;
    float bottomRadius;
    float3 groundAlbedo;
    float topRadius;
    float sunCosRadius;
    float aerialScale;
    float2 pad;
};

// Area lights' tables, the sun's cookie and the sky; mirrors GpuLighting.
struct Lighting
{
    uint ltcMatrix;
    uint ltcAmplitude;
    uint sunCookie;
    float sunCookieSize;
    float3 sunRight;
    uint sky;
    float3 sunUp;
    uint transmittance;
    uint multiScattering;
    uint skyView;
    uint aerialPerspective;
    // The tangent of half the sun's angle across, which softens its shadows; 0 for hard ones.
    float sunSoftness;
    Atmosphere atmosphere;
    // The colour the sun's shadows take, as a share of its light; black for none.
    float3 sunShadowTint;
    // 1 when the sky's light is the moon: its disc, and a night sky.
    uint moon;
};

// The level's wind; mirrors GpuWind in src/gpu.rs. Strength 0 without a Wind Zone.
struct Wind
{
    float2 direction;
    float strength;
    float gusts;
    float gustSize;
    float gustSpeed;
    float turbulence;
    float pad;
};

// What pushes through foliage this frame; mirrors GpuPushes in src/gpu.rs (see foliage.rs).
static const uint MAX_PUSHERS = 16;
static const uint PUSH_CELLS = 128;

struct Pushes
{
    // Each pusher's position and radius, and strength, four to a row.
    float4 pushers[MAX_PUSHERS];
    float4 strengths[MAX_PUSHERS / 4];
    // The grid's lowest cell, x and z.
    int2 origin;
    float cell;
    uint count;
};

// Ray-traced GI's probes; mirrors GpuDdgi in src/gpu.rs.
static const uint DDGI_CASCADES = 2;
static const float DDGI_CASCADE_SCALE = 4.0;
static const int3 DDGI_PROBES = int3(32, 16, 32);
static const uint DDGI_RAYS = 128;
static const uint DDGI_TEXELS = 8;

struct Ddgi
{
    int4 origin[DDGI_CASCADES];
    int4 previousOrigin[DDGI_CASCADES];
    float spacing;
    uint irradiance;
    uint depth;
    uint probes;
    uint enabled;
    uint scene;
    uint warmUp;
    float strength;
    float4 rotation[3];
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
    // How bright the stars are.
    float stars;
    Mat4 unjitteredViewProj;
    Mat4 prevViewProj;
    float2 jitter;
    uint taaReset;
    uint frameIndex;
    Post post;
    // How many meshlets each instance list's draws can turn into, at most;
    // the meshlet lists are laid out by it (meshletListBase).
    uint meshletCapacity;
    uint meshletFlags;
    uint2 pad5;
    Lighting lighting;
    Ddgi ddgi;
    Wind wind;
    Pushes pushes;
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
    // Pixels wide, or 0 for the pass's usual width.
    float width;
    float4 color;
};

// A node of a hierarchy for tracing rays without ray tracing (src/bvh.rs):
// its box, and its first child (the second follows) or, with count above 0,
// its first primitive in the hierarchy's order.
struct BvhNode
{
    float3 min;
    uint leftOrFirst;
    float3 max;
    uint count;
};

// An instance as the scene's hierarchy names it: the inverse of its model
// matrix's rows, taking rays into its mesh's space, its instance and mesh.
struct TraceInstance
{
    float4 inverse[3];
    uint instance;
    uint mesh;
    uint2 pad;
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
    Meshlet* meshlets;
    uint* meshletVertices;
    uint* meshletTriangles;
    DrawCommand* meshletDraws;
    uint* meshletDrawInstances;
    // The meshlets the cull queued for the meshlet pass, laid out as the
    // meshlet lists are: each as its draw (as drawInstances) and its place
    // in the draw's LOD.
    uint2* meshletWork;
    // Every LOD's indices, counting from its mesh's first vertex.
    uint* indices;
    // Without ray tracing: the meshes' hierarchies, the table of where each
    // mesh's start (nodes, order), and this frame's hierarchy of instances.
    BvhNode* bvhNodes;
    uint* bvhOrder;
    uint2* bvhTable;
    BvhNode* sceneNodes;
    TraceInstance* sceneInstances;
    // The grid of pushes through foliage, a push each cell.
    float2* foliagePushes;
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
    // DRAWS_INSTANCES or DRAWS_MESHLETS.
    uint source;
    uint4 textures;
};

[[vk::push_constant]] ConstantBuffer<Push> push;

Buffers* buffers()
{
    return push.buffers;
}

// A draw's instance, from the instance list or the meshlet list the pass
// draws, with the LOD it draws and its role in a fade.
uint drawInstance(uint drawId, out uint lod, out uint fade)
{
    uint packed = push.source == DRAWS_MESHLETS
        ? buffers()->meshletDrawInstances[meshletListBase(push.list, push.frame->meshletCapacity) + drawId]
        : buffers()->drawInstances[push.list * push.maxDraws + drawId];
    lod = (packed >> DRAW_LOD_SHIFT) & 0xF;
    fade = packed & (DRAW_FADING_IN | DRAW_FADING_OUT);
    return packed & DRAW_INSTANCE_MASK;
}

// A draw's first index, which names a meshlet (or a whole LOD) for as long
// as its mesh is loaded: the meshlets debug view colours by it.
uint drawFirstIndex(uint drawId)
{
    return push.source == DRAWS_MESHLETS
        ? buffers()->meshletDraws[meshletListBase(push.list, push.frame->meshletCapacity) + drawId].firstIndex
        : buffers()->draws[push.list * push.maxDraws + drawId].firstIndex;
}

// A bright colour of its own for each key, for debug views.
float3 keyColor(uint key)
{
    uint h = key * 2654435761u;
    h ^= h >> 15;
    h *= 2246822519u;
    h ^= h >> 13;
    return float3(uint3(h, h >> 8, h >> 16) & 0xFF) / 255.0 * 0.75 + 0.25;
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
