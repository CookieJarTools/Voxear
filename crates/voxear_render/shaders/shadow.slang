// Depth-only rendering of shadow casters into one cascade of the atlas, or
// one view of a local light's.

#include "custom.slang"
#include "wind.slang"

[shader("vertex")]
float4 shadowVertex(uint vertexId : SV_VertexID, uint drawId : SV_DrawIndex) : SV_Position
{
    uint lod;
    uint fade;
    Instance instance = buffers()->instances[drawInstance(drawId, lod, fade)];
    Mesh mesh = buffers()->meshes[instance.mesh];
    Vertex v = buffers()->vertices[mesh.vertexOffset + vertexId];
    float4 world = transform(instance.model, float4(v.position, 1.0));
    Material material = buffers()->materials[instance.material];
    float3 normal = normalize(instance.normal0.xyz * v.normal.x + instance.normal1.xyz * v.normal.y + instance.normal2.xyz * v.normal.z);
    float3 origin = instance.model.c3.xyz;
    world.xyz += windOffset(push.frame, material, origin, plantHeight(instance, mesh), world.xyz, normal, v.wind, push.frame->time);
    // A custom shader's vertex function moves its shadow too.
    if (material.shader != STANDARD_SHADER)
    {
        VertexInput moved = { world.xyz, normal, v.uv, origin, push.frame->time };
        world.xyz += customVertex(instance.material, moved);
    }
    return transform(shadowViewProj(push.frame, push.list, push.level), world);
}
