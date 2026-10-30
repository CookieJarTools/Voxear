// Depth-only rendering of shadow casters into one cascade of the atlas.

#include "common.slang"

[shader("vertex")]
float4 shadowVertex(uint vertexId : SV_VertexID, uint drawId : SV_DrawIndex) : SV_Position
{
    uint lod;
    Instance instance = push.instances[drawInstance(drawId, lod)];
    Mesh mesh = push.meshes[instance.mesh];
    Vertex v = push.vertices[mesh.vertexOffset + vertexId];
    float4 world = transform(instance.model, float4(v.position, 1.0));
    return transform(push.frame[0].cascadeViewProj[push.list - LIST_SHADOW], world);
}
