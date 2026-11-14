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
// reflections step through: each texel the largest (nearest, with reverse-Z)
// depth under it. Unlike the culling pyramid, it starts at the depth
// buffer's own size and each level is half the last rounded up, so every
// pixel is under one texel of each level; where the level above has an odd
// count, its last texel takes the extra row or column too.
// push.texture: the depth for the first level, else the level above as
// storage; push.textures.xy: the level above's size; push.output: this level.
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
        storageImagesR32f[push.output][id.xy] = textures[push.texture].Load(int3(id.xy, 0)).r;
        return;
    }
    uint2 above = push.textures.xy;
    uint2 first = id.xy * 2;
    uint2 last = min(first + 1 + uint2(id.xy == push.size - 1) * (above & 1), above - 1);
    float nearest = 0.0;
    for (uint y = first.y; y <= last.y; y++)
    {
        for (uint x = first.x; x <= last.x; x++)
        {
            nearest = max(nearest, storageImagesR32f[push.texture][uint2(x, y)]);
        }
    }
    storageImagesR32f[push.output][id.xy] = nearest;
}
