// The frame as every program of a shader pack sees it (src/shader_packs.rs's GpuPackFrame), shared by the passes'
// header (shader_pack.slang) and the engine's own shaders that run a pack's surface programs (shader_pack_surface.slang).

#pragma once

// What a block's face is, for a pack's surface programs (shader_pack_surface.slang names them BLOCK_LEAVES and the
// rest; blocks.slang's terrainShading sets them).
static const uint PACK_BLOCK_LEAVES = 1;
static const uint PACK_BLOCK_SHEET = 2;
static const uint PACK_BLOCK_PART = 4;
static const uint PACK_BLOCK_CUT_OUT = 8;
static const uint PACK_BLOCK_GLOWS = 16;
static const uint PACK_BLOCK_SMOOTH = 32;
static const uint PACK_BLOCK_CUBES = 64;

// What water is, for a pack's water program (shader_pack_surface.slang names them WATER_IS_SEA and the rest).
static const uint PACK_WATER_SEA = 1;
static const uint PACK_WATER_POOL = 2;
static const uint PACK_WATER_STREAM = 4;
static const uint PACK_WATER_BLOCKS = 8;
static const uint PACK_WATER_FROM_BELOW = 16;

// What a pixel's surface is, in a surface id's top four bits (depthFragment in occlusion.slang writes them): a model's,
// with its material's number below; a world of blocks' face, with what it is (PACK_BLOCK_*) and its look's number; a
// terrain's; and 0 for the sky.
static const uint PACK_SURFACE_MODEL = 1;
static const uint PACK_SURFACE_BLOCK = 2;
static const uint PACK_SURFACE_TERRAIN = 3;

// A surface id: the kind in its top four bits and what of it below.
uint packSurfaceId(uint kind, uint what)
{
    return (kind << 28) | (what & 0x0FFFFFFF);
}

static const uint PACK_TARGETS = 8;
static const uint PACK_OPTIONS = 32;
static const float PACK_PI = 3.14159265;

struct PackMatrix
{
    float4 c0;
    float4 c1;
    float4 c2;
    float4 c3;
};

float4 packTransform(PackMatrix m, float4 v)
{
    return m.c0 * v.x + m.c1 * v.y + m.c2 * v.z + m.c3 * v.w;
}

// The sun's light where the camera is after its way through the air, and the sky's light from all round.
struct PackSkyLight
{
    float4 sun;
    float4 sky;
};

// The sky light and block light kept for the blocks round the camera (src/blocks.rs's GpuLightVolume).
struct PackLightWindow
{
    int3 origin;
    uint found;
    float3 corner;
    float blockSize;
    float4 glow;
};

static const int3 PACK_LIGHT_WINDOW = int3(96, 48, 96);
static const uint PACK_LIGHT_SOLID = 0x0F;

// The frame as every program of a pack sees it. Directions are in the world, unit length; colours linear.
struct PackFrame
{
    // The camera's, as this frame is drawn: jittered as TAA jitters it, and last frame's.
    PackMatrix view;
    PackMatrix projection;
    PackMatrix viewProjection;
    PackMatrix inverseViewProjection;
    PackMatrix lastViewProjection;
    float3 cameraPosition;
    // Seconds since the level opened.
    float time;
    float3 cameraForward;
    float near;
    // How far TAA moves this frame's samples, in pixels of the scene as drawn.
    float2 jitter;
    // How large the scene is drawn, and how large the picture is shown, in pixels: the same but where TAA upscales.
    uint2 drawnSize;
    uint2 shownSize;
    uint frameIndex;
    // The day it is by the level's clock, from 0.
    uint day;
    // The way to the sky's light: the sun by day, the moon by night (`night` 1).
    float3 sunDirection;
    // How strong it is, as its light says.
    float sunStrength;
    // Its own colour, and the light it gives where the camera is: colour by strength, through the weather.
    float3 sunColor;
    // The time of day by the level's clock, 0 to 24.
    float timeOfDay;
    float3 sunLight;
    float night;
    // The weather now, each from 0 to 1: clouds, rain, snow, fog and wind, lightning's flash this moment, and the
    // ground as wet and as snowed over as it has made it.
    float cloud;
    float rain;
    float snow;
    float fog;
    float wind;
    float lightning;
    float wetness;
    float snowCover;
    // The way the wind blows, along x and z.
    float2 windDirection;
    // What the engine multiplies the scene's light by to show it, as last frame's exposure left it.
    float exposure;
    // What the bloom's sum is multiplied by, to be as bright as what glows (sceneBloom).
    float bloomScale;
    // The pack's options, in the order its pack.toml lists them: a number as it is, a toggle 0 or 1, a choice its
    // place among the choices.
    float4 options[PACK_OPTIONS / 4];
    // The frame's images, PACK_NO_TEXTURE where a pass reads none.
    uint depthTexture;
    uint surfaceTexture;
    uint motionTexture;
    uint lastColorTexture;
    // The pack's own targets this frame, and last frame's for those it keeps.
    uint targets[PACK_TARGETS];
    uint lastTargets[PACK_TARGETS];
    // What each pixel's surface is (sceneSurfaceId), PACK_NO_TEXTURE where no pass reads it.
    uint surfaceIdTexture;
    uint pad2;
    // The sun's and sky's light under the atmosphere; null under a sky of colours.
    PackSkyLight* skyLight;
    // The block light window; null where the camera is in no world of blocks lit by it.
    PackLightWindow* lightWindow;
    uint* lightBytes;
    // How much brighter than paper white the display shows its brightest, 1 where it is not an HDR display: a pass at
    // the tonemap may leave highlights above 1, up to it.
    float displayPeak;
    // The colour the sun takes through the leaves over a point, from its cascades, and the nearest's depth, for a
    // pack whose `[shadows] leaves` lets some of the sun through them; PACK_NO_TEXTURE without.
    uint leafLight;
};
