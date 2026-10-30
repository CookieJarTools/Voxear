// Clustered lighting: one thread per froxel tests every light's sphere
// against the froxel's view-space box and records the ones that reach it.
// Shading then loops over a pixel's froxel instead of every light.

#include "common.slang"

[shader("compute")]
[numthreads(64, 1, 1)]
void clusterMain(uint3 id : SV_DispatchThreadID)
{
    Frame* frame = push.frame;
    uint3 grid = frame->clusterGrid;
    if (id.x >= grid.x * grid.y * grid.z)
    {
        return;
    }
    uint3 cell = uint3(id.x % grid.x, (id.x / grid.x) % grid.y, id.x / (grid.x * grid.y));

    // Inverse of the slicing in forward.slang: slice = log(depth) * scale - bias.
    float nearDepth = exp((float(cell.z) + frame->clusterBias) / frame->clusterScale);
    float farDepth = exp((float(cell.z + 1) + frame->clusterBias) / frame->clusterScale);
    if (cell.z == grid.z - 1)
    {
        farDepth = 1e9;
    }

    // View-space box of the tile between the two depths. Vulkan NDC points Y down.
    float tanV = float(frame->viewport.y) / (2.0 * frame->lodScale);
    float tanH = tanV * float(frame->viewport.x) / float(frame->viewport.y);
    float2 ndcLow = float2(cell.xy) / float2(grid.xy) * 2.0 - 1.0;
    float2 ndcHigh = float2(cell.xy + 1) / float2(grid.xy) * 2.0 - 1.0;
    float2 slopeLow = float2(ndcLow.x * tanH, -ndcHigh.y * tanV);
    float2 slopeHigh = float2(ndcHigh.x * tanH, -ndcLow.y * tanV);
    float2 low = min(slopeLow * nearDepth, slopeLow * farDepth);
    float2 high = max(slopeHigh * nearDepth, slopeHigh * farDepth);
    float3 boxLow = float3(low, nearDepth);
    float3 boxHigh = float3(high, farDepth);

    uint base = id.x * (frame->clusterMaxLights + 1);
    uint count = 0;
    for (uint i = 0; i < frame->lightCount && count < frame->clusterMaxLights; i++)
    {
        Light light = buffers()->lights[i];
        float3 center = transform(frame->view, float4(light.position, 1.0)).xyz;
        float3 offset = center - clamp(center, boxLow, boxHigh);
        if (dot(offset, offset) <= light.range * light.range)
        {
            buffers()->clusters[base + 1 + count] = i;
            count++;
        }
    }
    buffers()->clusters[base] = count;

    uint ignored;
    InterlockedMax(buffers()->stats[STAT_CLUSTER_MAX_LIGHTS], count, ignored);
    InterlockedAdd(buffers()->stats[STAT_CLUSTER_LIGHT_LINKS], count, ignored);
}
