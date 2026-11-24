// Pixel's block faces: their light in flat bands over the shade colour (bands.slang).

#include "bands.slang"

float3 run(BlockSurface surface, BlockLighting light)
{
    float3 albedo = surface.color * (1.0 - surface.metallic * 0.5);
    return banded(albedo, light.sunTerm + light.lamps + light.ambientTerm, light.emission);
}
