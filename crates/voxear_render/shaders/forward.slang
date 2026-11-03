// The forward pass's vertex shader: each indirect draw finds its instance
// through the draw index the culling pass recorded. What it makes of a
// vertex, and the fragment shaders, are in shading.slang.

#include "shading.slang"

[shader("vertex")]
VertexOut vertexMain(uint vertexId : SV_VertexID, uint drawId : SV_DrawIndex)
{
    uint lod;
    uint fade;
    uint instanceIndex = drawInstance(drawId, lod, fade);
    uint cluster = push.frame->debugView == DEBUG_VIEW_MESHLETS ? drawFirstIndex(drawId) : 0;
    return forwardVertex(instanceIndex, lod, fade, cluster, vertexId);
}
