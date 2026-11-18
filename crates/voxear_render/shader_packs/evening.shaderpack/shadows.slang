// Evening's shadows: the engine's, paler the farther above what casts them is, as the evening sky's light fills a long
// shadow; a crown's shadow on the grass far below it is fainter than a wall's at its foot.

float run(ShadowSurface surface, ShadowLighting light)
{
    Options o = options();
    if (o.long_shadows <= 0.0 || light.engine >= 0.999)
    {
        return light.engine;
    }
    SunShadowMap map = sunShadowMap(surface, 0.0);
    float caster = shadowCaster(map, float2(0.0));
    float fade = smoothstep(2.0, 40.0, caster) * 0.5 * o.long_shadows;
    return lerp(light.engine, light.clouds, fade);
}
