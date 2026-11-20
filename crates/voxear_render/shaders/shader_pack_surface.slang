// What a shader pack's surface programs are written against: the face of a block they light (BlockSurface) or the
// water (WaterSurface), the light it sees as the engine works it out (BlockLighting, WaterLighting), and the frame as
// every program of a pack sees it. A block surface program defines
//
//     float3 run(BlockSurface surface, BlockLighting light)
//
// a models program
//
//     float3 run(ModelSurface surface, ModelLighting light)
//
// a water program
//
//     float3 run(WaterSurface surface, WaterLighting light)
//
// a sky program
//
//     float3 run(SkySurface surface, SkyLighting light)
//
// and a shadow filter
//
//     float run(ShadowSurface surface, ShadowLighting light)
//
// each returning the light leaving the surface towards the camera, before the air's haze and the clouds in front of
// it. Voxear compiles them into its own shaders that draw block faces (blocks.slang, with VOXEAR_PACK_BLOCKS defined)
// and water (water.slang, with VOXEAR_PACK_WATER), with a file `surface_wrapper` in src/shader_packs.rs writes on the
// include path, each program in a namespace of its own; they read only what is here, so they keep working as the
// engine's own shaders change.

#pragma once

#include "shader_pack_frame.slang"

// What a face is, as BlockSurface.kind has it, a bit each.
// A face of leaves, or a sheet of leaves or of a vine.
static const uint BLOCK_LEAVES = PACK_BLOCK_LEAVES;
// A sheet of a bushy block's leaves or of a vine, standing in the block, not one of its faces.
static const uint BLOCK_SHEET = PACK_BLOCK_SHEET;
// A face of a block of parts, as a door's or a chest's.
static const uint BLOCK_PART = PACK_BLOCK_PART;
// Its picture is cut out where it is clear, as leaves' are.
static const uint BLOCK_CUT_OUT = PACK_BLOCK_CUT_OUT;
// It gives off light of its own.
static const uint BLOCK_GLOWS = PACK_BLOCK_GLOWS;
// Of a world drawn smooth rather than as cubes.
static const uint BLOCK_SMOOTH = PACK_BLOCK_SMOOTH;
// Cut into small cubes, as a look pack can have every face.
static const uint BLOCK_CUBES = PACK_BLOCK_CUBES;

// The face of a block a surface program lights, at one pixel. Places and ways are in the world; colours are linear.
struct BlockSurface
{
    float3 position;
    // The way it faces with its picture's bumps, and the way the face itself faces.
    float3 normal;
    float3 faceNormal;
    // The way from it to the camera.
    float3 view;
    // Its colour, after its biome's tint, snow and the wet; how rough and how metallic it is; and the light it gives
    // off.
    float3 color;
    float alpha;
    float roughness;
    float metallic;
    float3 emission;
    // How open its corner is, from 0 where blocks close it in to 1.
    float shade;
    // Its sky light and block light, each from 0 to 1, smooth across it from its corners.
    float skyLight;
    float blockLight;
    // Its look's number among its world's, and what it is (BLOCK_LEAVES and the rest).
    uint look;
    uint kind;
    // Where it shows in the picture, in pixels.
    float2 pixel;
};

// The light a face sees, as the engine works it out: what reaches it, and the engine's own terms of its colour.
struct BlockLighting
{
    // The way to the sky's light, the sun by day and the moon by night, and its light as it arrives, through the air,
    // glass and water, before its shadow.
    float3 sunDirection;
    float3 sunLight;
    // How much of that reaches the face, from 0 in shadow to 1: its shadow, and in a cave as its sky light lets in.
    float shadow;
    // The sky's light from all round reaching the face, as much as its sky light lets in, and the blocks' light
    // reaching it, as much as its block light says.
    float3 sky;
    float3 glow;
    // How much of the light from all round reaches it, from its corner's shade and ambient occlusion, by colour: light
    // bouncing in a corner takes the colour of what it bounces off.
    float3 occlusion;
    // The light it reflects of its surroundings, along the way a mirror would.
    float3 reflection;
    // The engine's own terms, which add up to `engine`: the sun's light on it, shadow and all; the light from all
    // round on its colour, occluded; what it reflects of its surroundings; the lamps' light on it; and what it gives
    // off.
    float3 sunTerm;
    float3 ambientTerm;
    float3 reflectionTerm;
    float3 lamps;
    float3 emission;
    float3 engine;
};

// What a model's surface is, as ModelSurface.kind has it, a bit each.
// A foliage material's, lit through from behind as leaves are.
static const uint MODEL_FOLIAGE = PACK_MODEL_FOLIAGE;
// A material with a custom shader's own lighting; `engine` is what that lighting made.
static const uint MODEL_CUSTOM = PACK_MODEL_CUSTOM;
// It gives off light of its own.
static const uint MODEL_GLOWS = PACK_MODEL_GLOWS;

// A model's surface a models program lights, at one pixel: characters, props, creatures, anything drawn from a mesh.
// Places and ways are in the world; colours are linear.
struct ModelSurface
{
    float3 position;
    // The way it faces with its normal map's bumps, and the way its triangle faces.
    float3 normal;
    float3 faceNormal;
    // The way from it to the camera.
    float3 view;
    // Its colour, after the seasons and the wet; how rough and how metallic it is; and the light it gives off.
    float3 color;
    float alpha;
    float roughness;
    float metallic;
    float3 emission;
    // The sky light and block light round it, each from 0 to 1, in a world of blocks; 1 and 0 elsewhere.
    float skyLight;
    float blockLight;
    // Its material's number, the same for every model of that material, and what it is (MODEL_FOLIAGE and the rest).
    uint material;
    uint kind;
    // Where it shows in the picture, in pixels.
    float2 pixel;
};

// The light a model's surface sees: the same as a block face's.
typedef BlockLighting ModelLighting;

// What water is, as WaterSurface.kind has it, a bit each: the sea, a Pool, a Stream, a world of blocks' water, and
// whether it is seen from under it.
static const uint WATER_IS_SEA = PACK_WATER_SEA;
static const uint WATER_IS_POOL = PACK_WATER_POOL;
static const uint WATER_IS_STREAM = PACK_WATER_STREAM;
static const uint WATER_IS_BLOCKS = PACK_WATER_BLOCKS;
static const uint WATER_FROM_BELOW = PACK_WATER_FROM_BELOW;

// Water a water program lights, at one pixel. Places and ways are in the world; colours are linear.
struct WaterSurface
{
    // Where it is, the way its rippled surface faces there, and the way to the camera.
    float3 position;
    float3 normal;
    float3 view;
    // How far the view goes through it to what lies behind, in metres.
    float depth;
    // How much of it is foam or white water, from 0 to 1.
    float foam;
    // Its sky light and block light, each from 0 to 1: a world of blocks' water's own, else open sky.
    float skyLight;
    float blockLight;
    // What it is (WATER_IS_SEA and the rest).
    uint kind;
    // Where it shows in the picture, in pixels.
    float2 pixel;
};

// The light water sees, as the engine works it out, and the engine's own colour of it.
struct WaterLighting
{
    // The way to the sun (or the moon), its light as it arrives, and how much reaches the water, from 0 to 1.
    float3 sunDirection;
    float3 sunLight;
    float shadow;
    // The sky's light from all round reaching it.
    float3 sky;
    // How much it mirrors, head on less, glancing more; and what it mirrors there.
    float fresnel;
    float3 reflection;
    // What lies behind it, seen through it, as it would be without the water; how much of that comes through, by
    // colour; and the water's own light along the view into it.
    float3 behind;
    float3 through;
    float3 scatter;
    // The sun's and the lamps' glints on it.
    float3 glints;
    // The engine's colour of it without its foam, which is
    // (1 - fresnel) * (behind * through + scatter) + fresnel * reflection + glints, near enough; and with it.
    float3 clear;
    float3 engine;
};

// The sky a sky program draws, at one pixel.
struct SkySurface
{
    // The way from the camera through the pixel, in the world.
    float3 direction;
    // Where it shows in the picture, from 0 to 1 across and down, and in pixels.
    float2 uv;
    float2 pixel;
    // Whether the camera is under the clouds' base, so that the clouds are in front of the far weather, not behind.
    bool underClouds;
};

// The sky as the engine draws it that way, part by part, and the whole of it.
struct SkyLighting
{
    // The sky's own colour, without the sun's or the moon's disc, which is apart, and the stars.
    float3 sky;
    float3 disc;
    float3 stars;
    // The clouds in front of it and the weather hanging far off: what each adds, and in alpha how much of what is
    // behind shows through.
    float4 clouds;
    float4 weather;
    // All of it as the engine lays it: skyLayered(sky + disc + stars, clouds, weather, underClouds).
    float3 engine;
};

// A sky of a program's own with clouds and far weather laid in front of it, in the order the camera sees them.
float3 skyLayered(float3 behind, float4 clouds, float4 weather, bool underClouds)
{
    return underClouds ? (behind * clouds.a + clouds.rgb) * weather.a + weather.rgb
        : (behind * weather.a + weather.rgb) * clouds.a + clouds.rgb;
}

// A point a shadow filter reads the sun's shadow at.
struct ShadowSurface
{
    // Where it is, and the way the surface faces there, in the world.
    float3 position;
    float3 normal;
    // The way to the sun (or the moon), and how wide it is: the tangent of half the angle it takes up in the sky, 0
    // for a sun with no width. A shadow's soft edge is as wide as twice that by the distance to what casts it.
    float3 toSun;
    float sunWidth;
    // An angle that is different for each point and each frame, for turning taps round it, as TAA averages them.
    float turn;
};

// The sun's shadow there as the engine reads it, from 1 lit to 0 in shade.
struct ShadowLighting
{
    // Soft as the sun's width makes it, and with a hard edge; each under the clouds' shadow.
    float engine;
    float hard;
    // The clouds' shadow alone.
    float clouds;
    // The colour the glass between the point and the sun lets its light through, and with `[shadows] leaves` the
    // leaves too, their colour at its full brightness and halfway to white: white where there is none. Glass casts no dark shadow;
    // leaves do, and the sun a filter lets through them, which it says with lightThroughLeaves, lights blocks, models,
    // terrain and grass in their colour.
    float3 through;
};

// A shadow filter reads the sun's shadow map itself with these (shaders/shading.slang has them):
//
//     SunShadowMap sunShadowMap(ShadowSurface surface, float reach)  the map round the point, fine enough to read
//                                                                    `reach` metres round it
//     float shadowLit(SunShadowMap map, float2 offset)               1 lit, 0 in shade, `offset` metres across
//     float shadowCaster(SunShadowMap map, float2 offset)            metres up to what casts the shadow there, or 0
//     float2 shadowDisc(uint index, uint count, float turn)          points spread over a disc, for taps
//     void lightThroughLeaves(float amount)                          that much of what it returns came through leaves

// The frame as every program of the pack sees it.
PackFrame* frame()
{
    return buffers()->packFrame;
}

// The sun's light where the camera is, through the air, and the sky's light from all round.
float3 sunLight()
{
    PackSkyLight* light = frame()->skyLight;
    return light != nullptr ? light->sun.rgb : frame()->sunLight;
}

float3 skyAround()
{
    PackSkyLight* light = frame()->skyLight;
    return light != nullptr ? light->sky.rgb : frame()->sunLight * 0.1;
}

float luminance(float3 color)
{
    return dot(color, float3(0.2126, 0.7152, 0.0722));
}
