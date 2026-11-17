// A warm evening grade over the finished picture: light towards gold and shade towards a cool violet, kept as bright
// as they were; the darkest shade a little lifted, as evening air fills it; a gentle curve for depth; and a little
// more colour. Stronger as the sun sinks.

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
    float3 color = sceneColor(input.uv).rgb;
    float low = 1.0 - smoothstep(0.05, 0.6, f->sunDirection.y);
    float warmth = o.warmth * lerp(0.8, 1.0, low) * (1.0 - f->night * 0.6);

    float3 g = toGamma(color);
    float light = luma(g);
    float3 tint = lerp(float3(0.93, 0.96, 1.1), float3(1.16, 1.0, 0.72), smoothstep(0.1, 0.8, light));
    float3 toned = g * lerp(float3(1.0), tint, warmth);
    toned *= light / max(luma(toned), 1e-4);
    toned += (1.0 - toned) * float3(0.045, 0.03, 0.035) * warmth;
    float3 curved = toned * toned * (3.0 - 2.0 * toned);
    toned = lerp(toned, curved, 0.2 * warmth);
    float grey = luma(toned);
    toned = grey + (toned - grey) * (1.0 + 0.2 * warmth);
    // Above 1 only on an HDR display's highlights, which keep their brightness.
    return float4(fromGamma(max(toned, 0.0)), 1.0);
}
