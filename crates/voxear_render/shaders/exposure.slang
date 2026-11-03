// Auto exposure: meters a bloom level, an eighth of the screen across and
// already filtered, as a histogram of log luminance over a 64x64 grid,
// centre-weighted, and takes the average of the part of it between two
// percentiles, so it exposes for the brighter part of the picture and lets
// shadows stay dark, rather than lifting them as a plain average does. It
// then moves the adapted EV towards that target: steadily while far off,
// easing in over the last stop and a half, as Unreal's does. The composite
// pass reads the result; the stats show it too.

#include "common.slang"

static const float3 LUMA = float3(0.2126, 0.7152, 0.0722);
// The average brightness of a daylit scene; darker scenes are left darker.
static const float DAYLIGHT = 0.05;
// The histogram: its bins, and the range of log2 luminance they cover.
static const uint BINS = 64;
static const float LOG_LOWEST = -16.0;
static const float LOG_HIGHEST = 6.0;
// Within this many stops of its target, exposure eases in rather than moving steadily.
static const float EASE_STOPS = 1.5;

groupshared uint bins[BINS];

float binLog(uint bin)
{
    return LOG_LOWEST + (float(bin) + 0.5) / float(BINS) * (LOG_HIGHEST - LOG_LOWEST);
}

[shader("compute")]
[numthreads(16, 16, 1)]
void exposureMain(uint3 thread : SV_GroupThreadID)
{
    Frame* frame = push.frame;
    uint index = thread.y * 16 + thread.x;
    if (index < BINS)
    {
        bins[index] = 0;
    }
    GroupMemoryBarrierWithGroupSync();
    for (uint y = 0; y < 4; y++)
    {
        for (uint x = 0; x < 4; x++)
        {
            float2 uv = (float2(thread.xy * 4 + uint2(x, y)) + 0.5) / 64.0;
            float3 color = textures[push.textures.x].SampleLevel(samplers[SAMPLER_LINEAR_CLAMP], uv, 0.0).rgb * frame->exposure;
            // Pixels near the middle count for more, as the eye looks there.
            float weight = 1.5 - length(uv - 0.5);
            float place = (log2(max(dot(color, LUMA), 1e-9)) - LOG_LOWEST) / (LOG_HIGHEST - LOG_LOWEST);
            uint bin = uint(clamp(place * float(BINS), 0.0, float(BINS - 1)));
            InterlockedAdd(bins[bin], uint(weight * 64.0));
        }
    }
    GroupMemoryBarrierWithGroupSync();
    if (index != 0)
    {
        return;
    }

    Post post = frame->post;
    // The average of the bins between the two percentiles, each counted for as much of it as lies between them.
    float total = 0.0;
    for (uint bin = 0; bin < BINS; bin++)
    {
        total += float(bins[bin]);
    }
    float low = total * saturate(post.exposureLowPercent / 100.0);
    float high = total * saturate(max(post.exposureHighPercent, post.exposureLowPercent + 1.0) / 100.0);
    float below = 0.0;
    float2 sum = 0.0;
    for (uint bin = 0; bin < BINS; bin++)
    {
        float count = float(bins[bin]);
        float inside = max(min(below + count, high) - max(below, low), 0.0);
        sum += float2(binLog(bin) * inside, inside);
        below += count;
    }
    float average = exp2(sum.y > 0.0 ? sum.x / sum.y : -10.0);
    // An eye adapts less the darker it gets: each stop darker than daylight
    // the scene is, the picture is left a growing share of a stop darker than
    // mid-grey, so a moonlit night stays dark however far exposure may go.
    float belowDaylight = max(log2(DAYLIGHT / average), 0.0);
    // At most three stops, so the darkest scene still shows a moonlit shape.
    float darker = post.exposureNight * min(0.25 * pow(belowDaylight, 1.5), 3.0);
    float metered = log2(0.18 / average) - darker;
    float target = clamp(metered + post.exposureCompensation, post.exposureMin, post.exposureMax);
    float* state = buffers()->exposure;
    float current = state[0];
    // Straight to the target on the first frame, and while the bounced light is still settling as the GI starts,
    // so a level opens at its own exposure rather than sliding into it.
    if (state[1] == 0.0 || frame->gi.warmUp > 0)
    {
        current = target;
    }
    else
    {
        // Steadily while far off, at the speed in stops a second, then easing in over the last stops at a rate
        // that meets it, so the change neither lurches nor creeps. A lower target means the scene got brighter.
        float speed = target < current ? post.exposureSpeedUp : post.exposureSpeedDown;
        float gap = target - current;
        float step = speed * frame->deltaTime;
        current += abs(gap) > EASE_STOPS ? clamp(gap, -step, step) : gap * (1.0 - exp(-step / EASE_STOPS));
    }
    state[0] = current;
    state[1] = 1.0;
    state[2] = average;
}
