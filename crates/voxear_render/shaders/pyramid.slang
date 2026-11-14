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
// half the depth buffer's size, rounded up so each level halves exactly
// (src/ambient_occlusion.rs's pyramid_desc), with nothing past the depth's
// edge; the walk reads the depth itself for the finest level. push.texture:
// the depth for the first level, else the level above as storage;
// push.textures.xy: the depth's size; push.output: this level.
[shader("compute")]
[numthreads(8, 8, 1)]
void nearestMain(uint3 id : SV_DispatchThreadID)
{
    if (any(id.xy >= push.size))
    {
        return;
    }
    uint2 at = id.xy * 2;
    float nearest = 0.0;
    if (push.level == 0)
    {
        for (uint i = 0; i < 4; i++)
        {
            uint2 under = at + uint2(i & 1, i >> 1);
            if (all(under < push.textures.xy))
            {
                nearest = max(nearest, textures[push.texture].Load(int3(under, 0)).r);
            }
        }
    }
    else
    {
        RWTexture2D<float> above = storageImagesR32f[push.texture];
        nearest = max(max(above[at], above[at + uint2(1, 0)]), max(above[at + uint2(0, 1)], above[at + uint2(1, 1)]));
    }
    storageImagesR32f[push.output][id.xy] = nearest;
}
