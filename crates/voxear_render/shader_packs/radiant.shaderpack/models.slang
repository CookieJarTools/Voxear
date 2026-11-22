// Radiant's light on models, as on block faces: a warm sun and cool shade, darker deep in a cave, torchlight falling
// off quickly; grass, flowers and other foliage glowing with a low sun through them; and a warm rim round what stands
// against it. A material with a custom shader's own lighting keeps it.

float3 run(ModelSurface surface, ModelLighting light)
{
    if ((surface.kind & MODEL_CUSTOM) != 0)
    {
        return light.engine;
    }
    Options o = options();
    PackFrame* f = frame();
    float s = o.strength;
    float low = 1.0 - smoothstep(0.08, 0.7, light.sunDirection.y);
    float warmth = o.warmth * lerp(0.5, 1.0, low) * (1.0 - f->night * 0.7) * min(s, 1.5);
    float3 diffuse = surface.color * (1.0 - surface.metallic);

    // A low sun's light kept strong, as the evening's light reads: the engine's thins it towards the horizon.
    float3 sun = light.sunTerm * lerp(float3(1.0), float3(1.3, 1.02, 0.7), warmth) * (1.0 + 0.7 * low * min(s, 1.5));
    float deep = pow(saturate(surface.skyLight), 1.6 * o.cave_darkness * min(s, 1.0));
    float3 sky = light.sky * lerp(float3(1.0), float3(0.8, 0.92, 1.25), warmth) * deep;
    float near = saturate(surface.blockLight);
    float3 glow = light.glow * lerp(float3(1.0), float3(1.12, 0.94, 0.78), warmth) * o.torchlight
        * lerp(1.0, near * near * near * 2.2, saturate(s));
    float3 color = sun + (sky + glow) * diffuse * light.occlusion + light.reflectionTerm * lerp(1.0, deep, saturate(s))
        + light.lamps + light.emission;

    float against = saturate(dot(-surface.view, light.sunDirection));
    float day = 1.0 - f->night * 0.8;
    if ((surface.kind & MODEL_FOLIAGE) != 0)
    {
        // Thin blades and petals let the sun through: seen against it they glow, and a little from every side.
        float through = 0.25 + 0.75 * pow(against, 3.0);
        color += diffuse * light.sunLight * light.shadow * through * 0.45 * o.foliage_glow * s * day;
        return color;
    }
    // Against a low sun, the thin edges turned towards it catch its light, where it reaches them.
    float edge = pow(1.0 - saturate(dot(surface.normal, surface.view)), 5.0);
    float towards = saturate(dot(surface.normal, light.sunDirection) + 0.2);
    color += light.sunLight * diffuse * pow(against, 4.0) * edge * towards * light.shadow * low * 0.3 * s * day;
    return color;
}
