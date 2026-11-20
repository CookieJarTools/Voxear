// Vivid's sky: overhead a deeper, cleaner blue at the brightness the engine gives it, paler towards the horizon as a
// clear sky is; the engine's sun, stars, clouds and far weather laid over it as they are.

float3 run(SkySurface surface, SkyLighting light)
{
    Options o = options();
    PackFrame* f = frame();
    float up = saturate(surface.direction.y);
    // By day; at night the sky is the engine's.
    float day = (1.0 - f->night) * smoothstep(-0.05, 0.15, f->sunDirection.y);
    float3 blue = float3(0.13, 0.38, 1.0);
    float3 deep = luminance(light.sky) * blue / luminance(blue);
    float3 sky = lerp(light.sky, deep, saturate(pow(up, 0.6) * 0.75 * o.sky_blue * day));
    return skyLayered(sky + light.disc + light.stars, light.clouds, light.weather, surface.underClouds);
}
