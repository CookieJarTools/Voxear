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
    Vertex v = buffers()->vertices[instanceVertices(instance, mesh) + vertexId];
    float4 world = transform(instance.model, float4(v.position, 1.0));
    Material material = buffers()->materials[instance.material];
    float3 normal = normalize(instance.normal0.xyz * v.normal.x + instance.normal1.xyz * v.normal.y + instance.normal2.xyz * v.normal.z);
    float3 origin = instance.model.c3.xyz;
    world.xyz += windOffset(push.frame, material, origin, plantHeight(instance, mesh), world.xyz, normal, v.wind, push.frame->time);
    // A custom shader's vertex function moves its shadow too.
    if (material.shader != STANDARD_SHADER)
    {
        float3 level = levelPosition(push.frame, world.xyz, 4096.0);
        VertexInput moved = { world.xyz, normal, v.uv, origin, push.frame->time, level };
        world.xyz += customVertex(instance.material, moved);
    }
    return transform(shadowViewProj(push.frame, push.list, push.level), world);
}

// A view of the kept atlas of what stands still (src/shadow_cache.rs) copied
// into the same place in the local shadow atlas, for what moves to be drawn
// over: one triangle over the view, for the view push.level names, where
// something moving is drawn for its light this frame, and none elsewhere.
[shader("vertex")]
float4 keptCopyVertex(uint vertexId : SV_VertexID) : SV_Position
{
    uint slot = push.level / LOCAL_SHADOW_SLOT_VIEWS;
    uint cached = push.frame->localShadows.cached;
    bool moving = ((cached >> (KEPT_MOVING_SHIFT + slot)) & 1) != 0
        || buffers()->drawCount[drawCountIndex(LIST_LOCAL_SHADOW + slot)] != 0;
    if (!moving)
    {
        return float4(0.0, 0.0, 2.0, 1.0);
    }
    float2 corner = float2((vertexId << 1) & 2, vertexId & 2);
    return float4(corner * 2.0 - 1.0, 0.5, 1.0);
}

[shader("fragment")]
float keptCopyFragment(float4 position : SV_Position) : SV_Depth
{
    Texture2D kept = textures[push.frame->localShadows.cache];
    return kept.Load(int3(int2(position.xy), 0)).r;
}
