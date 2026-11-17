// What a shader pack's surface programs are written against: the face of a block they light (BlockSurface) or the
// water (WaterSurface), the light it sees as the engine works it out (BlockLighting, WaterLighting), and the frame as
// every program of a pack sees it. A block surface program defines
//
//     float3 run(BlockSurface surface, BlockLighting light)
//
// and a water program
//
//     float3 run(WaterSurface surface, WaterLighting light)
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
