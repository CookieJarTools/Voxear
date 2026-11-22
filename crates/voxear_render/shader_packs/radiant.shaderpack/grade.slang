// Radiant's own tonemap and grade, in place of the engine's: a gentle glow round what is bright; in a cave, the eye
// kept from adapting all the way to the dark, so a cave stays dark and an opening blazes; a filmic curve with deep
// blacks; then highlights towards a warm peach and shade towards a cool slate, kept as bright as they were; richer
// colour in the middle tones; and corners a touch darker.

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
    float s = o.strength;
    float3 light = sceneColor(input.uv).rgb;
    if (bloomDrawn())
    {
        light = lerp(light, sceneBloom(input.uv), saturate(0.08 * o.glow * max(s, 0.25)));
    }
    // Under cover, the exposure the eye adapted to held back towards the open sky's, as the sky light round the camera
    // says.
    float cave = hasBlockLight() ? 1.0 - smoothstep(0.15, 0.85, lightAround(f->cameraPosition).x) : 0.0;
    float exposure = f->exposure * lerp(1.0, 0.3, cave * o.cave_darkness * saturate(s));
    light *= exposure;
    float3 filmic = tonemapFilmic(light * 1.35);
    float3 color = lerp(tonemapSoft(light), filmic, saturate(s));

    float low = 1.0 - smoothstep(0.08, 0.7, f->sunDirection.y);
    float warmth = o.warmth * lerp(0.6, 1.0, low) * (1.0 - f->night * 0.6) * min(s, 1.5);
    float3 g = toGamma(color);
    float bright = luma(g);
    float3 shade = float3(0.9, 0.97, 1.1);
    float3 lit = float3(1.12, 1.0, 0.86);
    float3 tint = lerp(shade, lit, smoothstep(0.15, 0.85, bright));
    float3 toned = g * lerp(float3(1.0), tint, warmth);
    toned *= bright / max(luma(toned), 1e-4);
    // Richer colour where it is neither dark nor blown: the sunlit grass and flowers most.
    float grey = luma(toned);
    float middle = smoothstep(0.05, 0.35, grey) * (1.0 - smoothstep(0.75, 1.0, grey));
    toned = grey + (toned - grey) * (1.0 + 0.45 * s * middle);
    // A gentle curve, for depth.
    float3 curved = toned * toned * (3.0 - 2.0 * toned);
    toned = lerp(toned, curved, 0.18 * saturate(s));

    float aspect = float(f->shownSize.x) / float(f->shownSize.y);
    float2 d = (input.uv - 0.5) * float2(aspect, 1.0);
    float corner = length(float2(0.5 * aspect, 0.5));
    toned *= 1.0 - 0.35 * o.vignette * pow(saturate(length(d) / corner), 2.2);
    return float4(saturate(fromGamma(toned)), 1.0);
}
