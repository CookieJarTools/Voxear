// Pixel's tonemap, in place of the engine's: the light exposed and fitted to the screen by 1 - exp(-light), each
// colour on its own, nothing else, so the bands the surface programs step for come out as flat as they were made.

float4 run(PassInput input)
{
    float3 light = max(sceneColor(input.uv).rgb, 0.0);
    return float4(1.0 - exp(-light * frame()->exposure), 1.0);
}
