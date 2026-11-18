// Evening's own tonemap and grade, in place of the engine's: a little glow round what is bright, the light exposed
// and fitted to the screen by the curve the options choose, then light towards gold and shade towards a cool violet,
// kept as bright as they were; the darkest shade a little lifted, as evening air fills it; a gentle curve for depth;
// a little more colour; and corners a touch darker. Stronger as the sun sinks.

float3 toGamma(float3 c)
{
    return pow(max(c, 0.0), 1.0 / 2.2);
}

float3 fromGamma(float3 c)
{
    return pow(max(c, 0.0), 2.2);
}

float luma(float3 c)
{
    return dot(c, float3(0.2126, 0.7152, 0.0722));
}

float4 run(PassInput input)
{
    Options o = options();
    PackFrame* f = frame();
    float3 light = sceneColor(input.uv).rgb;
    if (bloomDrawn())
    {
        light = lerp(light, sceneBloom(input.uv), 0.05 * o.glow);
    }
    light *= f->exposure;
    float3 color = o.curve == CURVE_NEUTRAL ? tonemapNeutral(light)
        : o.curve == CURVE_FILMIC ? tonemapFilmic(light)
        : tonemapSoft(light, 1.05, 1.1);

    float low = 1.0 - smoothstep(0.05, 0.6, f->sunDirection.y);
    float warmth = o.warmth * lerp(0.8, 1.0, low) * (1.0 - f->night * 0.6);
    float3 g = toGamma(color);
    float bright = luma(g);
    float3 tint = lerp(float3(0.93, 0.96, 1.1), float3(1.16, 1.0, 0.72), smoothstep(0.1, 0.8, bright));
    float3 toned = g * lerp(float3(1.0), tint, warmth);
    toned *= bright / max(luma(toned), 1e-4);
    toned += (1.0 - toned) * float3(0.045, 0.03, 0.035) * warmth;
    float3 curved = toned * toned * (3.0 - 2.0 * toned);
    toned = lerp(toned, curved, 0.2 * warmth);
    float grey = luma(toned);
    toned = grey + (toned - grey) * (1.0 + 0.2 * warmth);

    float aspect = float(f->shownSize.x) / float(f->shownSize.y);
    float2 d = (input.uv - 0.5) * float2(aspect, 1.0);
    float corner = length(float2(0.5 * aspect, 0.5));
    toned *= 1.0 - 0.3 * o.vignette * pow(saturate(length(d) / corner), 2.5);
    return float4(saturate(fromGamma(toned)), 1.0);
}
