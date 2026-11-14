// Builds one level of the depth pyramid: each texel is the minimum
// (farthest, with reverse-Z) depth of the 2x2 texels under it, read with the
// min-reduction sampler in a single fetch.

#include "common.slang"

[shader("compute")]
[numthreads(8, 8, 1)]
void reduceMain(uint3 id : SV_DispatchThreadID)
{
    if (any(id.xy >= push.size))
    {
        return;
    }
    float2 uv = (float2(id.xy) + 0.5) / float2(push.size);
    float depth = textures[push.texture].SampleLevel(samplers[SAMPLER_MIN_REDUCTION], uv, float(push.level)).r;
    storageImagesR32f[push.output][id.xy] = depth;
}

// Builds one level of the nearest-depth pyramid that screen-space
// reflections walk through: each texel the largest (nearest, with reverse-Z)
// depth of the 2x2 under it. Unlike the culling pyramid, its first level is
// the depth buffer's own size, rounded up so each level halves exactly
// (src/ambient_occlusion.rs's pyramid_desc), with nothing past the depth's
// edge. push.texture: the depth for the first level, else the level above as
// storage; push.textures.xy: the size of what it reads; push.output: this
// level.
[shader("compute")]
[numthreads(8, 8, 1)]
void nearestMain(uint3 id : SV_DispatchThreadID)
{
    if (any(id.xy >= push.size))
    {
        return;
    }
    if (push.level == 0)
    {
        bool inside = all(id.xy < push.textures.xy);
        storageImagesR32f[push.output][id.xy] = inside ? textures[push.texture].Load(int3(id.xy, 0)).r : 0.0;
        return;
    }
    RWTexture2D<float> above = storageImagesR32f[push.texture];
    uint2 at = id.xy * 2;
    float nearest = max(max(above[at], above[at + uint2(1, 0)]), max(above[at + uint2(0, 1)], above[at + uint2(1, 1)]));
    storageImagesR32f[push.output][id.xy] = nearest;
}
