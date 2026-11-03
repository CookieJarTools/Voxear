// Meshlet culling's mesh shader for shadow casters, where the GPU has mesh
// shaders: each meshlet the task shader (meshlet_task.slang) hands it, depth
// only, as shadow.slang draws casters.

#include "culling.slang"

struct ShadowVertex
{
    float4 position : SV_Position;
};

// Casters into the cascade of list push.list.
[shader("mesh")]
[numthreads(MESHLET_ROUND, 1, 1)]
[outputtopology("triangle")]
void shadowMesh(
    uint3 group : SV_GroupID,
    uint3 thread : SV_GroupThreadID,
    in payload MeshletPayload payload,
    out vertices ShadowVertex vertices[MESHLET_VERTICES],
    out indices uint3 triangles[MESHLET_TRIANGLES])
{
    MeshletDrawn drawn = meshletDrawn(payload, group.x);
    uint vertexCount = drawn.meshlet.counts & 0xFF;
    uint triangleCount = drawn.meshlet.counts >> 8;
    SetMeshOutputCounts(vertexCount, triangleCount);
    uint vertexOffset = buffers()->meshes[drawn.data.mesh].vertexOffset;
    for (uint i = thread.x; i < vertexCount; i += MESHLET_ROUND)
    {
        Vertex v = buffers()->vertices[vertexOffset + buffers()->meshletVertices[drawn.vertexBase + i]];
        float4 world = transform(drawn.data.model, float4(v.position, 1.0));
        vertices[i].position = transform(push.frame->cascadeViewProj[push.list - LIST_SHADOW], world);
    }
    for (uint i = thread.x; i < triangleCount; i += MESHLET_ROUND)
    {
        triangles[i] = meshletTriangle(drawn, i);
    }
}
