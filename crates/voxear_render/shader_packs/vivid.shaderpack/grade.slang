// Vivid's own tonemap and grade, in place of the engine's: a little glow round what is bright, the light exposed and
// fitted to the screen with a firmer curve, then vibrance (the dullest colours made richest, those rich already left
// nearly as they are, so skin and sky do not turn garish), sunlit colours a little warmer, shade a little deeper and
// cooler, and corners darker.

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
    float3 color = tonemapSoft(light * f->exposure, 1.0 + 0.2 * o.contrast, 1.0 + 0.35 * o.vibrance);

    // In the picture's own gamma, where a step in colour is a step to the eye.
    float3 g = pow(max(color, 0.0), 1.0 / 2.2);
    float grey = luma(g);
    float most = max(g.r, max(g.g, g.b));
    float least = min(g.r, min(g.g, g.b));
    float saturation = (most - least) / max(most, 1e-4);
    float lift = 1.6 * o.vibrance * (1.0 - saturation) * smoothstep(0.02, 0.25, grey);
    g = grey + (g - grey) * (1.0 + lift);
    // Sunlit colours warmer, shade deeper and a touch cooler.
    float bright = smoothstep(0.35, 0.9, grey);
    g *= lerp(lerp(float3(0.96, 0.98, 1.04), float3(1.0), 0.5 + 0.5 * bright), float3(1.1, 1.0, 0.84), bright * o.warmth);
    g = lerp(g, g * g * (3.0 - 2.0 * g), 0.35 * o.contrast);

    float aspect = float(f->shownSize.x) / float(f->shownSize.y);
    float2 d = (input.uv - 0.5) * float2(aspect, 1.0);
    float corner = length(float2(0.5 * aspect, 0.5));
    g *= 1.0 - 0.45 * o.vignette * pow(saturate(length(d) / corner), 2.2);
    return float4(saturate(pow(max(g, 0.0), 2.2)), 1.0);
}
