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
