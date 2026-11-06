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
// One list for each local light casting a shadow, drawn into each of its views.
static const uint LIST_LOCAL_SHADOW = LIST_SHADOW + CASCADES;
static const uint LOCAL_SHADOW_LIGHTS = 8;
static const uint LIST_COUNT = LIST_LOCAL_SHADOW + LOCAL_SHADOW_LIGHTS;

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
// shadow cascade's and local light's holds one. `capacity` is the most meshlets every
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
    // How the wind moves it, a byte each: leaf flutter, its own phase, branch sway (wind.slang).
    uint wind;
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
// Leaves that stay as made through the seasons (seasons.slang).
static const uint MATERIAL_LEAVES_STAY = 16;
// How its glow moves, in two bits: 0 steady, 1 pulse, 2 breathe, 3 flicker (glow.slang).
static const uint MATERIAL_GLOW_SHIFT = 5;
// A plant's impostor, its base colour and normal textures its atlas (impostor.slang).
static const uint MATERIAL_IMPOSTOR = 128;

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
    // How many times a second its glow goes round, when it moves (glow.slang).
    float glowSpeed;
    // Its custom shader's number, or STANDARD_SHADER; its properties are
    // MATERIAL_PROPERTIES float4s of Buffers' materialProperties.
    uint shader;
    // How far its custom shader's vertex function may move a vertex, in
    // metres, which culling allows for.
    float maxOffset;
    uint2 pad;
};

static const uint STANDARD_SHADER = 0;
static const uint MATERIAL_PROPERTIES = 16;

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
    // From how far from the camera it fades out, by dithering, and how far it is gone; y 0 for never.
    // z: a plant's tint, RGBA8 in its bits, 0 for none.
    float4 fade;
};

// An instance whose place in the list another Obj held last frame.
static const uint INSTANCE_NEW = 1;
// Where an instance's forced LOD, plus one, sits in its flags.
static const uint INSTANCE_LOD_SHIFT = 8;
// An instance whose LODs switch by how much of the screen it covers, at its lodScreen.
static const uint INSTANCE_LOD_SCREEN = 2;
// An instance that casts no shadow, as grass far from the camera.
static const uint INSTANCE_NO_SHADOW = 4;
// A plant's impostor, its quad turned to the camera (impostor.slang).
static const uint INSTANCE_IMPOSTOR = 16;

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
    // Its first view in the local shadow atlas, or NO_SHADOW: a spot light's one, or a cube's six faces.
    uint shadow;
    uint3 pad;
};

static const uint NO_SHADOW = 0xFFFFFFFF;

// Local lights' shadows; mirrors GpuLocalShadows in src/gpu.rs. Views are
// LOCAL_SHADOW_TILE square, LOCAL_SHADOW_COLUMNS to a row of the atlas.
static const uint LOCAL_SHADOW_VIEWS = 48;
static const uint LOCAL_SHADOW_TILE = 512;
static const uint LOCAL_SHADOW_COLUMNS = 8;
static const uint2 LOCAL_SHADOW_ATLAS = uint2(4096, 3072);
// The glass shadow atlas (src/glass.rs): the sun's cascades in its top
// GLASS_SUN_SIZE square, laid out as the shadow atlas lays them, and the
// local lights' views under it, each GLASS_LOCAL_TILE square, as the local
// shadow atlas lays them.
static const uint2 GLASS_SHADOW_ATLAS = uint2(1024, 1792);
static const uint GLASS_SUN_SIZE = 1024;
static const uint GLASS_LOCAL_TILE = 128;
// The sea (src/ocean.rs, GpuOcean): its cascades' tiles in metres, their images (each point's movement and
// how it folds; its slopes and their squares; last frame's movement), its colours, linear, and how fast each
// colour is lost through it, a metre; its level, how sharp its crests are, how high its waves run (their
// significant height) and the time its waves are at, which repeats.
struct Ocean
{
    // Each cascade's tile, in metres, and its images (OCEAN_CASCADES of them, src/ocean.rs's CASCADES).
    float sizes[6];
    uint displacement[6];
    uint slopes[6];
    uint previous[6];
    float3 deep;
    float level;
    float3 scatter;
    float foam;
    float3 absorb;
    float choppiness;
    float waveHeight;
    uint enabled;
    float time;
    // How many Pools, then Dry Areas, Buffers::waterBodies holds; its Streams come between them.
    uint pools;
    uint dries;
    // The foam round the camera (water.slang's oceanFoamMain), and where its first texel's corner is.
    uint foamTexture;
    float2 foamOrigin;
    uint streams;
    // The sea floor round the camera (water.slang's floorVertex): its depth image, and each slice's corner, size and
    // whether it is drawn.
    uint floorTexture;
    // The tiling pattern foam is made of (water.slang's foamPatternMain).
    uint foamPattern;
    // Which way the wind blows, as an angle from +x towards +z, in radians.
    float wind;
    float4 floor[4];
    // How deep the sea is round each texel of the sea floor, blurred (water.slang's floorDepthMain), and which of its
    // costlier looks are on (src/ocean.rs's features).
    uint floorDepth;
    uint features;
    // Where the Dry Areas shaped as meshes start and end along each pixel's view (water.slang's dryMeshFragment), or
    // NO_TEXTURE.
    uint dryMask;
    // How much higher every wave is than its spectrum (src/ocean.rs's SeaState::scale).
    float waveScale;
};

// A place along a Stream (src/gpu.rs, GpuStreamPoint): its surface in the middle, which way it runs, how wide it is,
// how fast it flows and how much it foams.
struct StreamPoint
{
    float3 position;
    float width;
    float3 tangent;
    float speed;
    float foam;
    // How long what flows there has taken to come from the Stream's start, in seconds.
    float age;
    // How far into its mouth it is, where it runs into the sea or a Pool, and how high that water stands
    // (src/water_bodies.rs's run_into).
    float mouth;
    float meets;
};

// A Pool or a Dry Area (src/water_bodies.rs, GpuWaterBody): where it is, a Pool's footprint (x, z) or a Dry
// Area's size, its kind (WATER_POOL, WATER_DRY, WATER_STREAM), a Pool's colours, waves and liquid, a Dry Area's
// shape. A Stream's points are world-space, so its matrices are the identity; its depth is size.y, and its first
// point in Buffers::streamPoints is in shape.
static const uint WATER_POOL = 1;
static const uint WATER_DRY = 2;
static const uint WATER_STREAM = 3;
// A Sphere or Box Wind Zone, kept among the Dry Areas: the sea's short waves inside it as rough as `waves` says,
// fading out over lava.x metres outside it (src/wind.rs).
static const uint WATER_WIND = 4;
// What fills a Pool or a Stream (Liquid in src/water_bodies.rs).
static const uint LIQUID_WATER = 0;
static const uint LIQUID_LAVA = 1;
static const uint DRY_BOX = 0;
static const uint DRY_SPHERE = 1;
static const uint DRY_CAPSULE = 2;
static const uint DRY_MESH = 3;
struct WaterBody
{
    Mat4 toWorld;
    Mat4 toLocal;
    float3 size;
    uint kind;
    float3 deep;
    uint shape;
    float3 scatter;
    float waves;
    float3 absorb;
    uint liquid;
    // Lava's: how many seconds it takes to crust over (x), and how brightly it glows (y).
    float4 lava;
};

struct LocalShadows
{
    Mat4 viewProj[LOCAL_SHADOW_VIEWS];
    // Each shadow list's light, and how far its shadow reaches.
    float4 casters[LOCAL_SHADOW_LIGHTS];
    uint texture;
    uint count;
    uint2 pad;
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
    // How much specular anti-aliasing roughens shiny surfaces whose normals turn fast across a pixel, 0 for none.
    float specularSmoothing;
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
    // Each pixel's normal and roughness from the depth passes (NO_TEXTURE without them), for TAA to tell mirrors by.
    uint surfaceTexture;
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
    // How far the camera moved since last frame: 0 still, 1 a few centimetres or a fraction of a degree or more.
    float cameraMotion;
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

// The level's seasons for its plants; mirrors GpuSeasons in src/gpu.rs (see seasons.slang).
struct Seasons
{
    // Each season's leaf colour, weighted by how far leaves turn to it, and that amount;
    // spring, summer, autumn and winter.
    float4 colors[4];
    // Each season's snow, in y.
    float4 looks[4];
    // The colour dry leaves turn, and how much of a field dries.
    float4 dry;
    float time;
    float variation;
    float patchSize;
    float wetness;
    uint enabled;
    // How much of the snow and wetness reach what is not a plant.
    float ground;
    uint2 pad;
};

// The sun's light where the camera is, after its way through the air, and
// the sky's light from all round, worked out once a frame (atmosphere.slang).
struct SkyLight
{
    float4 sun;
    float4 sky;
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
static const uint DDGI_CASCADES = 3;
static const float DDGI_CASCADE_SCALE = 2.0;
static const int3 DDGI_PROBES = int3(32, 16, 32);
static const uint DDGI_RAYS = 256;
static const uint DDGI_TEXELS = 8;
// The probes' light is kept and blended raised to 1/DDGI_GAMMA, as RTXGI keeps it (Majercik et al. 2021): one
// frame's few rays that meet a bright sliver, a candle's flame or a neon tube's glass, then move a probe a little,
// where blended as light they moved it a lot, and lamp-lit walls shimmered with it.
static const float DDGI_GAMMA = 5.0;

float3 ddgiEncode(float3 light)
{
    return pow(max(light, 0.0), 1.0 / DDGI_GAMMA);
}

float3 ddgiDecode(float3 kept)
{
    return pow(max(kept, 0.0), DDGI_GAMMA);
}

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
    // The glass shadow atlas's light and smoothness (glassLight), each its
    // texture's index plus one: 0, as frames from before it wrote, for none.
    uint2 glassShadow;
    Lighting lighting;
    Ddgi ddgi;
    Wind wind;
    Pushes pushes;
    Seasons seasons;
    LocalShadows localShadows;
    Ocean ocean;
};

// The projection a shadow draw puts its vertices through: a cascade's, or
// for a local light's list the view `level` names.
Mat4 shadowViewProj(Frame* frame, uint list, uint view)
{
    return list >= LIST_LOCAL_SHADOW ? frame->localShadows.viewProj[view] : frame->cascadeViewProj[list - LIST_SHADOW];
}

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
    // Pixels wide, or 0 for the pass's usual width; or with DEBUG_IN_METRES
    // (debug.slang), metres wide.
    float width;
    float4 color;
};

// A triangle of debug draw's solid shapes. Mirrors `GpuDebugTriangle` in
// src/gpu.rs.
struct DebugTriangle
{
    float3 a;
    uint flags;
    float3 b;
    uint pad;
    float3 c;
    uint pad2;
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
    // Under the atmosphere, the sun's light and the sky's, once a frame (atmosphere.slang's SkyLight); else null.
    SkyLight* skyLight;
    // Each material's custom shader's properties, MATERIAL_PROPERTIES a material.
    float4* materialProperties;
    // The sea's spectrum, each wave's h0(k) and h0*(-k), N×N a cascade; and its FFT's rows, four fields each.
    float4* oceanSpectrum;
    float2* oceanFft;
    // This frame's Pools, Streams, then Dry Areas; and the points along its Streams.
    WaterBody* waterBodies;
    StreamPoint* streamPoints;
    DebugTriangle* debugTriangles;
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


// `v` at length 1, or `otherwise` where it has none to speak of (or is not
// a number), which normalize would make NaN, and bloom then spread over
// the whole picture.
float3 normalizeOr(float3 v, float3 otherwise)
{
    float lengthSquared = dot(v, v);
    return lengthSquared > 1e-12 ? v * rsqrt(lengthSquared) : otherwise;
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
