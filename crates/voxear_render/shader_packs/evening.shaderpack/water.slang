// Evening's light on water: the sun's path across it golden, what it mirrors warmer the nearer it is to the sun, the
// water's own colour a little greener in its shade, and its foam as the engine has it.

float3 run(WaterSurface surface, WaterLighting light)
{
    Options o = options();
    PackFrame* f = frame();
    float low = 1.0 - smoothstep(0.05, 0.6, light.sunDirection.y);
    float warmth = o.warmth * lerp(0.6, 1.0, low) * (1.0 - f->night * 0.7);
    float3 gold = lerp(float3(1.0), float3(1.3, 1.0, 0.62), warmth);
    float toward = pow(saturate(dot(reflect(-surface.view, surface.normal), light.sunDirection)), 6.0);
    float3 mirrored = light.reflection * lerp(float3(1.0), gold, toward);
    float3 under = light.behind * light.through + light.scatter * lerp(float3(1.0), float3(0.88, 1.0, 0.94), warmth);
    float3 clear = (1.0 - light.fresnel) * under + light.fresnel * mirrored + light.glints * gold;
    // What the engine lays over the clear water, its foam and white water, laid over this as it is.
    return clear + (light.engine - light.clear);
}
