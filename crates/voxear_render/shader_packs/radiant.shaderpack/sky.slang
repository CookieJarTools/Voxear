// Radiant's sky: round a low sun, the sky overhead turns a soft lavender and the horizon a warm peach, its brightness
// kept; the sun is a large bright disc in a wide glow; and the clouds turn pink, lit from below on their undersides,
// gold-edged towards the sun. By day with a high sun only a little of it shows.

float3 run(SkySurface surface, SkyLighting light)
{
    Options o = options();
    PackFrame* f = frame();
    float3 d = surface.direction;
    float s = o.strength;
    float low = 1.0 - smoothstep(0.05, 0.6, f->sunDirection.y);
    float day = 1.0 - f->night;
    float evening = s * low * day;
    float toSun = saturate(dot(d, f->sunDirection));
    float up = saturate(d.y);
    float horizon = 1.0 - saturate(abs(d.y) * 2.5);

    float3 sky = light.sky;
    float bright = luminance(sky);
    float3 lavender = float3(0.86, 0.72, 0.92);
    float3 peach = float3(1.0, 0.74, 0.52);
    float3 tone = lerp(lavender, peach, saturate(horizon * 0.8 + pow(toSun, 4.0) * 0.6));
    float shift = saturate(evening * (0.55 + 0.25 * o.warmth));
    sky = lerp(sky, bright * tone / luminance(tone), shift);
    // A clear day's sky a touch deeper overhead.
    sky *= lerp(float3(1.0), float3(0.92, 0.97, 1.06), s * (1.0 - low) * up);

    // The sun's glow, wide and warm, and its disc, larger than the engine's and bright enough to bloom.
    float3 sun = sunLight() * day;
    float3 warmWhite = lerp(float3(1.0, 0.95, 0.85), float3(1.0, 0.8, 0.55), o.warmth * low);
    float glow = o.sun_glow * s;
    sky += sun * warmWhite * (pow(toSun, 1500.0) * 0.8 + pow(toSun, 200.0) * 0.12 + pow(toSun, 24.0) * 0.03 + pow(toSun, 4.0) * 0.012 * (0.5 + low)) * glow;
    float angle = acos(min(dot(d, f->sunDirection), 1.0));
    float disc = 1.0 - smoothstep(0.03, 0.04, angle);
    float3 discLight = sun * warmWhite * disc * 6.0 * min(glow, 1.0);

    float4 clouds = light.clouds;
    // Clouds near a low sun: pink, their sunward sides gold, their undersides lit warm from the sky below them.
    float3 pink = float3(1.0, 0.8, 0.94);
    float3 edge = float3(1.2, 0.96, 0.82);
    float3 tint = lerp(pink * 0.92, edge, pow(toSun, 4.0));
    float cover = 1.0 - clouds.a;
    clouds.rgb *= lerp(float3(1.0), tint, saturate(evening));
    clouds.rgb += sun * peach * cover * pow(toSun, 2.0) * 0.02 * evening;
    // The clouds hide the sun's disc where they are thick.
    return skyLayered(sky + light.disc * (1.0 - disc * min(glow, 1.0)) + discLight + light.stars, clouds, light.weather,
        surface.underClouds);
}
