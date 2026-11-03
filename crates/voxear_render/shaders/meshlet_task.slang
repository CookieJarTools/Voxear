// Meshlet culling with task and mesh shaders, where the GPU has them: the
// same culling as the meshlet pass in cull.slang, but this task shader culls
// each round of queued meshlets and hands the survivors straight to the mesh
// shaders (meshlets.slang), so no draw lists are written.
//
// Each shader of the path is a file of its own, as GLSL makes each stage, so
// each SPIR-V module holds one payload: NVIDIA's driver mixed up the payloads
// of a module with several, and mesh shaders read an empty one. Only GPUs
// with mesh shaders load them.

#include "culling.slang"

groupshared MeshletPayload survivors;
groupshared uint survivorCount;

// One workgroup for each round of meshlets the cull queued in list
// push.list, drawn with the size the cull grew for it.
[shader("amplification")]
[numthreads(MESHLET_ROUND, 1, 1)]
void meshletTask(uint3 group : SV_GroupID, uint3 thread : SV_GroupThreadID)
{
    Frame* frame = push.frame;
    uint list = push.list;
    if (thread.x == 0)
    {
        survivorCount = 0;
    }
    GroupMemoryBarrierWithGroupSync();
    bool tested;
    uint2 entry;
    MeshLod lod;
    uint result = cullQueued(frame, list, group.x * MESHLET_ROUND + thread.x, tested, entry, lod);
    if (push.source != DRAWS_AGAIN)
    {
        countMeshlets(list, tested, result);
    }
    if (tested && result == MESHLET_VISIBLE)
    {
        uint slot;
        InterlockedAdd(survivorCount, 1, slot);
        survivors.draws[slot] = entry.x;
        survivors.meshlets[slot] = entry.y;
    }
    GroupMemoryBarrierWithGroupSync();
    DispatchMesh(survivorCount, 1, 1, survivors);
}
