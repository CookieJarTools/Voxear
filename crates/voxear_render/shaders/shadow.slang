// Depth-only rendering of shadow casters into one cascade of the atlas.

#include "common.slang"

[shader("vertex")]
float4 shadowVertex(uint vertexId : SV_VertexID, uint drawId : SV_DrawIndex) : SV_Position
{
    uint lod;
    uint fade;
    Instance instance = buffers()->instances[drawInstance(drawId, lod, fade)];
    Mesh mesh = buffers()->meshes[instance.mesh];
    Vertex v = buffers()->vertices[mesh.vertexOffset + vertexId];
    float4 world = transform(instance.model, float4(v.position, 1.0));
    return transform(push.frame->cascadeViewProj[push.list - LIST_SHADOW], world);
}
