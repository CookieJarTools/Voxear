// Evening's light on models, as on block faces: the sun warmer and the shade under the sky a little cooler, and a
// warm rim of light round what stands against a low sun. A material with a custom shader's own lighting keeps it.

float3 run(ModelSurface surface, ModelLighting light)
{
    if ((surface.kind & MODEL_CUSTOM) != 0)
    {
        return light.engine;
    }
    Options o = options();
    PackFrame* f = frame();
    float low = 1.0 - smoothstep(0.05, 0.6, light.sunDirection.y);
    float warmth = o.warmth * lerp(0.6, 1.0, low) * (1.0 - f->night * 0.7);
    float3 sun = light.sunTerm * lerp(float3(1.0), float3(1.22, 1.0, 0.74), warmth);
    float3 diffuse = surface.color * (1.0 - surface.metallic);
    float3 sky = light.sky * lerp(float3(1.0), float3(0.84, 0.94, 1.22), warmth);
    float3 glow = light.glow * lerp(float3(1.0), float3(1.15, 0.95, 0.75), warmth);
    float3 color = sun + (sky + glow) * diffuse * light.occlusion + light.reflectionTerm + light.lamps + light.emission;
    // Against a low sun, the thin edges turned towards it catch its light, where it reaches them.
    float against = pow(saturate(dot(-surface.view, light.sunDirection)), 4.0);
    float edge = pow(1.0 - saturate(dot(surface.normal, surface.view)), 5.0);
    float towards = saturate(dot(surface.normal, light.sunDirection) + 0.2);
    color += light.sunLight * diffuse * against * edge * towards * light.shadow * low * 0.25 * o.rim;
    return color;
}
