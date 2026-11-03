// Meshlet culling's forward mesh shader, where the GPU has mesh shaders: it
// makes each meshlet the task shader (meshlet_task.slang) hands it, its
// vertices and triangles, which the forward pass's fragment shaders shade as
// they shade the vertex shader's (shading.slang). Shadow casters have their
// own (meshlet_shadow.slang).

#include "shading.slang"
#include "culling.slang"

[shader("mesh")]
[numthreads(MESHLET_ROUND, 1, 1)]
[outputtopology("triangle")]
void forwardMesh(
    uint3 group : SV_GroupID,
    uint3 thread : SV_GroupThreadID,
    in payload MeshletPayload payload,
    out vertices VertexOut vertices[MESHLET_VERTICES],
    out indices uint3 triangles[MESHLET_TRIANGLES])
{
    MeshletDrawn drawn = meshletDrawn(payload, group.x);
    uint vertexCount = drawn.meshlet.counts & 0xFF;
    uint triangleCount = drawn.meshlet.counts >> 8;
    SetMeshOutputCounts(vertexCount, triangleCount);
    // The first index the vertex shader path's meshlet draw has, for the same colour.
    uint cluster = push.frame->debugView == DEBUG_VIEW_MESHLETS ? drawn.meshLod.indexOffset + drawn.meshlet.firstIndex : 0;
    for (uint i = thread.x; i < vertexCount; i += MESHLET_ROUND)
    {
        uint vertexIndex = buffers()->meshletVertices[drawn.vertexBase + i];
        vertices[i] = forwardVertex(drawn.instance, drawn.lod, drawn.fade, cluster, vertexIndex);
    }
    for (uint i = thread.x; i < triangleCount; i += MESHLET_ROUND)
    {
        triangles[i] = meshletTriangle(drawn, i);
    }
}
