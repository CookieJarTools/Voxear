// GPU frustum culling: one thread per instance writes an indirect draw for
// every instance whose bounding sphere touches the view. The CPU never sees
// visibility. Occlusion culling against the depth pyramid builds on this.

#include "common.slang"

[shader("compute")]
[numthreads(64, 1, 1)]
void cullMain(uint3 id : SV_DispatchThreadID)
{
    Frame frame = push.frame[0];
    if (id.x >= frame.instanceCount)
    {
        return;
    }

    Instance instance = push.instances[id.x];
    Mesh mesh = push.meshes[instance.mesh];
    float3 center = transform(instance.model, float4(mesh.center, 1.0)).xyz;
    float scale = max(length(instance.model.c0.xyz), max(length(instance.model.c1.xyz), length(instance.model.c2.xyz)));
    float radius = mesh.radius * scale;

    // Five planes: the far plane is at infinity.
    for (int i = 0; i < 5; i++)
    {
        if (dot(frame.planes[i].xyz, center) + frame.planes[i].w < -radius)
        {
            return;
        }
    }

    uint slot;
    InterlockedAdd(push.drawCount[0], 1, slot);
    if (slot >= push.maxDraws)
    {
        return;
    }

    DrawCommand draw;
    draw.indexCount = mesh.indexCount;
    draw.instanceCount = 1;
    draw.firstIndex = mesh.indexOffset;
    draw.vertexOffset = 0;
    draw.firstInstance = 0;
    push.draws[slot] = draw;
    push.drawInstances[slot] = id.x;
}
