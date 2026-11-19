// Skinning (src/skinning.rs): every skinned part's vertices moved by their bones, each by up to four, weighed,
// into its posed vertices, in one dispatch. Each thread group poses 64 vertices of one part, its job among the
// frame's (skinGroups); groups past 65535 across go in rows. push.size.x: how many groups there are.
//
// Each vertex also says how much of it is bound to hidden bones, in the top byte of its wind masks, which only
// plants' own vertices use; the passes that draw what a camera sees leave out what is mostly hidden (shading.slang).

#include "common.slang"

[shader("compute")]
[numthreads(64, 1, 1)]
void skinMain(uint3 group : SV_GroupID, uint3 thread : SV_GroupThreadID)
{
    uint at = group.y * 65535 + group.x;
    if (at >= push.size.x)
    {
        return;
    }
    SkinJob job = buffers()->skinJobs[buffers()->skinGroups[at]];
    uint index = (at - job.firstGroup) * 64 + thread.x;
    if (index >= job.count)
    {
        return;
    }
    Vertex v = buffers()->vertices[job.source + index];
    float hidden;
    float3x4 moved = skinnedMatrix(buffers()->skinMatrices, job.matrices, buffers()->skins[job.skin + index], hidden);
    v.wind = (v.wind & 0x00FFFFFF) | (uint(saturate(hidden) * 255.0 + 0.5) << 24);
    float3x3 turn = float3x3(moved[0].xyz, moved[1].xyz, moved[2].xyz);
    v.position = mul(moved, float4(v.position, 1.0));
    v.normal = normalizeOr(mul(turn, v.normal), v.normal);
    v.tangent.xyz = normalizeOr(mul(turn, v.tangent.xyz), v.tangent.xyz);
    buffers()->posedVertices[job.output + index] = v;
}
