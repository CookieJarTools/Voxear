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

// Where a surface's pixel is, for `surface`: its UV, its place and normal
// in the world, the way to the camera, the Obj's place, and the time in
// seconds.
struct SurfaceInput
{
    float2 uv;
    float3 position;
    float3 normal;
    float3 view;
    float3 objectPosition;
    float time;
};

// A surface as `surface` sets it, starting as the material's own: its
// colour and alpha, its normal in the world, metallic, roughness and what
// it gives off.
struct SurfaceOutput
{
    float3 color;
    float alpha;
    float3 normal;
    float metallic;
    float roughness;
    float3 emission;
};

// A vertex, for `vertex`: its place and normal in the world, its UV, the
// Obj's place and the time in seconds.
struct VertexInput
{
    float3 position;
    float3 normal;
    float2 uv;
    float3 objectPosition;
    float time;
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
