// Evening's light on block faces: the sun warmer and the shade under the sky a little cooler, the light of lamps and
// torches kept in the blocks warmer still, and leaves lit through from behind when the sun is low beyond them.

float3 run(BlockSurface surface, BlockLighting light)
{
    Options o = options();
    PackFrame* f = frame();
    float low = 1.0 - smoothstep(0.05, 0.6, light.sunDirection.y);
    float warmth = o.warmth * lerp(0.6, 1.0, low) * (1.0 - f->night * 0.7);
    float3 sun = light.sunTerm * lerp(float3(1.0), float3(1.22, 1.0, 0.74), warmth);
    float3 diffuse = surface.color * (1.0 - surface.metallic);
    float3 sky = light.sky * lerp(float3(1.0), float3(0.84, 0.94, 1.22), warmth);
    float3 glow = light.glow * lerp(float3(1.0), float3(1.15, 0.95, 0.75), warmth);
    float3 color = sun + (sky + glow) * diffuse * light.occlusion + light.reflectionTerm + light.lamps + light.emission;
    if ((surface.kind & BLOCK_LEAVES) != 0)
    {
        // Seen against the sun, leaves glow with the light through them, most where the sky is open over them.
        float against = pow(saturate(dot(-surface.view, light.sunDirection)), 8.0);
        float open = surface.skyLight * surface.skyLight;
        color += diffuse * light.sunLight * against * open * 0.18 * o.leaf_glow * (1.0 - f->night * 0.8);
    }
    return color;
}
