// What a project's custom shaders (.shader assets, voxear_engine::shaders)
// are written with: the attributes their properties take, a picture they
// sample, and what their functions take and give. The project's shaders
// themselves, each in a namespace of its own, and the functions that call
// the one a material picks, are in project_shaders.slang, which the engine
// writes and compiles with VOXEAR_CUSTOM_SHADERS defined; without, the
// functions below call nothing.

#pragma once

#include "common.slang"

// [Property]: a field a material sets, shown in its Inspector.
[__AttributeUsage(_AttributeTargets.Var)]
struct PropertyAttribute {};
// [Color]: a float3 or float4 property picked as an sRGB colour.
[__AttributeUsage(_AttributeTargets.Var)]
struct ColorAttribute {};
// [Range(min, max)]: the least and most a number property may be.
[__AttributeUsage(_AttributeTargets.Var)]
struct RangeAttribute
{
    float low;
    float high;
};
// [Tooltip("...")]: what a property does, when the pointer rests on it.
[__AttributeUsage(_AttributeTargets.Var)]
struct TooltipAttribute
{
    string text;
};
// [Unlit] on a shader's struct: drawn as its surface's colour and emission alone.
[__AttributeUsage(_AttributeTargets.Struct)]
struct UnlitAttribute {};
// [MaxOffset(metres)] on a shader's struct: how far its vertex function may move a vertex.
[__AttributeUsage(_AttributeTargets.Struct)]
struct MaxOffsetAttribute
{
    float metres;
};

// A picture property: a picture asset the material picks.
struct Picture
{
    uint index;

    // Its colour at `uv`, wrapping round, as linear light.
    float4 sample(float2 uv)
    {
        return index == NO_TEXTURE ? float4(1.0) : sampleTexture(index, uv);
    }
};

// How thick a surface that is not Glass counts as: as good as bottomless.
static const float SOLID_THICKNESS = 1000.0;

// Where a surface's pixel is, for `surface`: its UV, its place and normal
// in the world, the way to the camera, the Obj's place, and the time in
// seconds. For a Glass material, `glass` is true and `thickness` is how far
// the view goes through it, in metres, to what lies behind: shallow water
// at a shore, deep in the middle. For any other, SOLID_THICKNESS. Last,
// its place measured from the level's middle, wrapped every 4096 metres:
// patterns read from it rather than from `position` stay where they are as
// the world's middle moves with the camera, and as exact far out.
struct SurfaceInput
{
    float2 uv;
    float3 position;
    float3 normal;
    float3 view;
    float3 objectPosition;
    float time;
    float thickness;
    bool glass;
    float3 levelPosition;
};

// A surface as `surface` sets it, starting as the material's own: its
// colour and alpha, its normal in the world, metallic, roughness and what
// it gives off; how much it reflects head on, if not metal (0.04 for most
// things, 0.02 for water); for Glass, how much of what lies behind shows
// through, by colour, its own colour being light it scatters, as murky
// water does; and four values of the shader's own for its `light` to read.
struct SurfaceOutput
{
    float3 color;
    float alpha;
    float3 normal;
    float metallic;
    float roughness;
    float3 emission;
    float reflectance;
    float3 transmission;
    float4 data;
};

// A surface as the material makes it, for its custom shader to change.
SurfaceOutput materialSurface(float3 color, float alpha, float3 normal, float metallic, float roughness, float3 emission)
{
    SurfaceOutput surface = { color, alpha, normal, metallic, roughness, emission, 0.04, float3(0.0), float4(0.0) };
    return surface;
}

// A vertex, for `vertex`: its place and normal in the world, its UV, the
// Obj's place and the time in seconds; and its place measured from the
// level's middle, wrapped every 4096 metres, as SurfaceInput's.
struct VertexInput
{
    float3 position;
    float3 normal;
    float2 uv;
    float3 objectPosition;
    float time;
    float3 levelPosition;
};

// A light falling on a surface, for `light`: the way to it, its colour as
// it arrives there (shadowed, and through any glass), and the way to the
// camera.
struct LightInput
{
    float3 direction;
    float3 color;
    float3 view;
};

#ifdef VOXEAR_CUSTOM_SHADERS
#include "project_shaders.slang"
#else
void customSurface(uint material, SurfaceInput input, inout SurfaceOutput surface) {}

float3 customVertex(uint material, VertexInput input)
{
    return 0.0;
}

bool customUnlit(uint material)
{
    return false;
}

bool customLighting(uint material)
{
    return false;
}

float3 customLight(uint material, LightInput light, SurfaceOutput surface)
{
    return 0.0;
}
#endif
