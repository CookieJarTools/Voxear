// Exposure and filmic tonemapping from the HDR target into the output. The
// output is sRGB, so the hardware applies the transfer curve on write.

#include "common.slang"

[shader("vertex")]
float4 tonemapVertex(uint vertexId : SV_VertexID) : SV_Position
{
    return float4(fullscreenNdc(vertexId), 0.0, 1.0);
}

// Narkowicz's fit of the ACES filmic curve.
float3 aces(float3 x)
{
    return saturate((x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14));
}

[shader("fragment")]
float4 tonemapFragment(float4 position : SV_Position) : SV_Target
{
    float3 hdr = textures[push.texture].Load(int3(int2(position.xy), 0)).rgb;
    return float4(aces(hdr * push.frame->exposure), 1.0);
}
