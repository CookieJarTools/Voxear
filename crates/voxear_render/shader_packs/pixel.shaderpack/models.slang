// Pixel's models: their light in flat bands over the shade colour, as block faces (bands.slang). A material with a
// custom shader's own lighting keeps it.

#include "bands.slang"

float3 run(ModelSurface surface, ModelLighting light)
{
    if ((surface.kind & MODEL_CUSTOM) != 0)
    {
        return light.engine;
    }
    float3 albedo = surface.color * (1.0 - surface.metallic * 0.5);
    return banded(albedo, light.sunTerm + light.lamps + light.ambientTerm, light.emission);
}
