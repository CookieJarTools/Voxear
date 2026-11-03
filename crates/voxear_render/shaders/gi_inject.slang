// Filling the voxel GI's clipmap (gi.slang) each frame, in three compute
// passes after the opaque passes:
// - shift: last frame's voxels moved to this frame's origins, as the camera
//   moved, and faded a little, so what has gone fades out;
// - shadows: every few texels of the sun's shadow cascades, a voxel where
//   the sun sees a surface, so walls out of view still block and bounce;
// - view: every other pixel the camera sees, its lit colour into its voxel,
//   which is the light that surface sends on.

#include "common.slang"

// How much of last frame's voxels stays, each frame.
static const float GI_FADE = 0.9;
// What a surface the sun lights sends on, for those only the sun sees:
// a mid-grey surface, facing the sun half on.
static const float GI_SUNLIT = 0.08;

int2 giTexel(int3 voxel, uint level)
{
    return int2(voxel.x + voxel.z * GI_SIZE, voxel.y + int(level) * GI_SIZE);
}

// textures: x last frame's voxels.
[shader("compute")]
[numthreads(8, 8, 1)]
void giShiftMain(uint3 id : SV_DispatchThreadID)
{
    Frame* frame = push.frame;
    if (id.x >= uint(GI_SIZE * GI_SIZE) || id.y >= uint(GI_SIZE) * GI_LEVELS)
    {
        return;
    }
    uint level = id.y / uint(GI_SIZE);
    int3 voxel = int3(int(id.x) % GI_SIZE, int(id.y) % GI_SIZE, int(id.x) / GI_SIZE);
    int3 before = voxel + frame->gi.origin[level].xyz - frame->gi.previousOrigin[level].xyz;
    float4 kept = 0.0;
    bool inside = all(before >= 0) && all(before < GI_SIZE);
    if (frame->gi.reset == 0 && inside)
    {
        kept = textures[NonUniformResourceIndex(push.textures.x)].Load(int3(giTexel(before, level), 0)) * GI_FADE;
    }
    storageImagesRgba16f[push.output][int2(id.xy)] = kept;
}

// Marks the voxel at a point, in each grid holding it. `colour` is written
// unless something brighter is there already, from the camera.
void giMark(Frame* frame, float3 position, float3 colour, bool keepBrighter)
{
    for (uint level = 0; level < GI_LEVELS; level++)
    {
        float size = frame->gi.voxelSize * float(1u << level);
        int3 voxel = int3(floor(position / size)) - frame->gi.origin[level].xyz;
        if (any(voxel < 0) || any(voxel >= GI_SIZE))
        {
            continue;
        }
        int2 texel = giTexel(voxel, level);
        if (keepBrighter && storageImagesRgba16f[push.output][texel].a > 0.95)
        {
            continue;
        }
        storageImagesRgba16f[push.output][texel] = float4(colour, 1.0);
    }
}

// One thread for every fourth texel each way of the shadow atlas's four cascades.
[shader("compute")]
[numthreads(8, 8, 1)]
void giShadowsMain(uint3 id : SV_DispatchThreadID)
{
    Frame* frame = push.frame;
    if (frame->shadowTexture == NO_TEXTURE)
    {
        return;
    }
    uint2 atlasSize;
    textures[NonUniformResourceIndex(frame->shadowTexture)].GetDimensions(atlasSize.x, atlasSize.y);
    uint2 texel = id.xy * 4u;
    if (any(texel >= atlasSize))
    {
        return;
    }
    float depth = textures[NonUniformResourceIndex(frame->shadowTexture)].Load(int3(int2(texel), 0)).r;
    if (depth >= 0.999)
    {
        return;
    }
    uint2 half = atlasSize / 2u;
    uint cascade = (texel.x >= half.x ? 1u : 0u) + (texel.y >= half.y ? 2u : 0u);
    float2 uv = (float2(texel % half) + 0.5) / float2(half);
    float4 world = transform(frame->gi.cascadeInverse[cascade], float4(uv * 2.0 - 1.0, depth, 1.0));
    giMark(frame, world.xyz / world.w, frame->sunColor * GI_SUNLIT, true);
}

// One thread for every other pixel each way. textures: x the scene as the
// opaque passes drew it, y depth.
[shader("compute")]
[numthreads(8, 8, 1)]
void giViewMain(uint3 id : SV_DispatchThreadID)
{
    Frame* frame = push.frame;
    int2 pixel = int2(id.xy * 2u);
    if (any(pixel >= int2(frame->viewport)))
    {
        return;
    }
    float depth = textures[NonUniformResourceIndex(push.textures.y)].Load(int3(pixel, 0)).r;
    if (depth <= 0.0)
    {
        return;
    }
    float2 uv = (float2(pixel) + 0.5) / float2(frame->viewport);
    float4 world = transform(frame->invViewProj, float4(uv * 2.0 - 1.0, depth, 1.0));
    float3 colour = textures[NonUniformResourceIndex(push.textures.x)].Load(int3(pixel, 0)).rgb;
    // Highlights and emissive glare would light the room as lamps; a lit surface sends on at most this.
    float brightest = max(colour.r, max(colour.g, colour.b));
    colour *= min(1.0, 4.0 / max(brightest, 1e-4));
    giMark(frame, world.xyz / world.w, colour, false);
}
