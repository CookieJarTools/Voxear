// Evening's sky: towards gold near the horizon and round a low sun, and a deeper blue overhead, its brightness kept;
// a glow round the sun; and the clouds lit gold on the sun's side. The engine's disc, stars and far weather stay.

float3 run(SkySurface surface, SkyLighting light)
{
    Options o = options();
    PackFrame* f = frame();
    float3 d = surface.direction;
    float low = 1.0 - smoothstep(0.05, 0.6, f->sunDirection.y);
    float strength = o.sky_glow * low * (1.0 - f->night);
    float toSun = saturate(dot(d, f->sunDirection));
    float horizon = 1.0 - saturate(abs(d.y) * 3.0);

    float3 gold = float3(1.0, 0.62, 0.3);
    float3 sky = light.sky;
    float warm = strength * saturate(horizon * 0.6 + pow(toSun, 6.0) * 0.8);
    sky = lerp(sky, luminance(sky) * gold / luminance(gold), 0.7 * saturate(warm));
    sky *= lerp(float3(1.0), float3(0.85, 0.9, 1.05), strength * saturate(d.y));
    sky += sunLight() * gold * pow(toSun, 24.0) * 0.15 * strength;

    float4 clouds = light.clouds;
    clouds.rgb *= lerp(float3(1.0), float3(1.25, 0.95, 0.7), saturate(strength * (0.4 + 0.6 * pow(toSun, 3.0))));
    return skyLayered(sky + light.disc + light.stars, clouds, light.weather, surface.underClouds);
}
