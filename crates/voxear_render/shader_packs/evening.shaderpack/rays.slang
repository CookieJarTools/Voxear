// Rays through leaves: from each pixel, the picture is walked towards the sun, and the sky round the sun seen along
// the way adds up. Where leaves, trunks and blocks stand in front of it nothing adds, so the light falls in shafts
// from every gap. Written at half size, one value of how much light; the next pass colours it.

static const int STEPS = 40;

float4 run(PassInput input)
{
    Options o = options();
    PackFrame* f = frame();
    if (!o.rays || f->sunStrength <= 0.0)
    {
        return 0.0;
    }
    bool inFront;
    float2 sun = toPicture(f->cameraPosition + f->sunDirection * 1.0e4, inFront);
    float facing = dot(f->cameraForward, f->sunDirection);
    if (!inFront || facing <= 0.0)
    {
        return 0.0;
    }
    // Each step a little further towards the sun, started a little way along by noise that moves each frame, so few
    // steps look like many once TAA has blended them.
    float2 step = (sun - input.uv) / float(STEPS);
    float2 uv = input.uv + step * frameNoise(input.pixel);
    float light = 0.0;
    float weight = 1.0;
    for (int i = 0; i < STEPS; i++)
    {
        if (all(uv >= 0.0) && all(uv <= 1.0) && isSky(uv))
        {
            // The sky's glow round the sun, as bright as the sky there is, cut off where clouds hide the sun.
            float round = pow(saturate(dot(viewRay(uv), f->sunDirection)), 48.0);
            float seen = saturate(luminance(sceneColor(uv).rgb) / max(luminance(sunLight()) * 0.02, 1e-4));
            light += round * seen * weight;
        }
        weight *= 0.96;
        uv += step;
    }
    // Strongest looking into the sun, gone as it turns to the side.
    float strength = smoothstep(0.0, 0.6, facing) * (1.0 - f->night * 0.7) * (1.0 - f->cloud * 0.8);
    return float4(light / float(STEPS) * strength * 4.0, 0.0, 0.0, 1.0);
}
