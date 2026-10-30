// Auto exposure: meters a bloom level, an eighth of the screen across and
// already filtered (centre-weighted average of log luminance over a 64x64
// grid), then moves the adapted EV towards the target at the configured
// speeds. The composite pass reads the result; the stats show it too.

#include "common.slang"

static const float3 LUMA = float3(0.2126, 0.7152, 0.0722);

groupshared float2 partial[256];

[shader("compute")]
[numthreads(16, 16, 1)]
void exposureMain(uint3 thread : SV_GroupThreadID)
{
    Frame* frame = push.frame;
    float2 sum = 0.0;
    for (uint y = 0; y < 4; y++)
    {
        for (uint x = 0; x < 4; x++)
        {
            float2 uv = (float2(thread.xy * 4 + uint2(x, y)) + 0.5) / 64.0;
            float3 color = textures[push.textures.x].SampleLevel(samplers[SAMPLER_LINEAR_CLAMP], uv, 0.0).rgb * frame->exposure;
            float weight = 1.5 - length(uv - 0.5);
            sum += float2(log2(max(dot(color, LUMA), 1e-5)) * weight, weight);
        }
    }
    uint index = thread.y * 16 + thread.x;
    partial[index] = sum;
    GroupMemoryBarrierWithGroupSync();
    for (uint stride = 128; stride > 0; stride >>= 1)
    {
        if (index < stride)
        {
            partial[index] += partial[index + stride];
        }
        GroupMemoryBarrierWithGroupSync();
    }
    if (index != 0)
    {
        return;
    }

    Post post = frame->post;
    float average = exp2(partial[0].x / partial[0].y);
    float target = clamp(log2(0.18 / average) + post.exposureCompensation, post.exposureMin, post.exposureMax);
    float* state = buffers()->exposure;
    float current = state[0];
    if (state[1] == 0.0)
    {
        current = target;
    }
    else
    {
        // A lower target means the scene got brighter.
        float speed = target < current ? post.exposureSpeedUp : post.exposureSpeedDown;
        current += clamp(target - current, -speed * frame->deltaTime, speed * frame->deltaTime);
    }
    state[0] = current;
    state[1] = 1.0;
    state[2] = average;
}
