// Evening's shadows: the engine's, paler the farther above what casts them is, as the evening sky's light fills a long
// shadow, so a crown's shadow on the grass far below it is fainter than a wall's at its foot; and under leaves a little
// of the sun comes through them, which the shading tints their colour.

float run(ShadowSurface surface, ShadowLighting light)
{
    Options o = options();
    if (light.engine >= 0.999)
    {
        return light.engine;
    }
    float lit = light.engine;
    if (o.long_shadows > 0.0)
    {
        SunShadowMap map = sunShadowMap(surface, 0.0);
        float caster = shadowCaster(map, float2(0.0));
        lit = lerp(lit, light.clouds, smoothstep(2.0, 40.0, caster) * 0.5 * o.long_shadows);
    }
    // Leaves above: their colour is far from white. A little of the sun comes through them, in their colour.
    float3 through = light.through;
    float leaves = saturate((1.0 - min(min(through.r, through.g), through.b)) * 8.0);
    float leak = (light.clouds - lit) * leaves * 0.2 * o.leaf_light;
    lightThroughLeaves(leak);
    return lit + leak;
}
