// Pixel's sky: the engine's, sun, clouds and all, its brightness stepped into twice as many flat bands as the light
// on surfaces, so a gradient sky shows in stripes; below the first, the shade colour, as a night sky.

float skyLuma(float3 c)
{
    return dot(c, float3(0.2126, 0.7152, 0.0722));
}

float3 run(SkySurface surface, SkyLighting light)
{
    Options o = options();
    float exposure = max(frame()->exposure, 1e-6);
    float3 shown = 1.0 - exp(-light.engine * exposure);
    float bright = skyLuma(shown);
    float steps = 2.0 * max(round(o.bands), 1.0);
    float stepped = floor(bright * steps + 0.5) / steps;
    shown = stepped > 0.0 ? shown * stepped / max(bright, 1e-4) : o.shade;
    shown = clamp(shown, 0.0, 0.985);
    return -log(1.0 - shown) / exposure;
}
