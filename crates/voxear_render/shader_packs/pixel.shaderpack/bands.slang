// Pixel's light in flat bands, shared by its block faces and its models. The light reaching a surface is measured as
// a white surface would show it on screen in its brightest channel, through the pack's own curve (grade.slang), so a
// deep orange lamp is as bright as a white one, and stepped into `bands` flat steps; the surface shows its colour in its light's colour at that step, over the shade colour. What it gives off is
// added as it is.

float bandLuma(float3 c)
{
    return dot(c, float3(0.2126, 0.7152, 0.0722));
}

// The scene's light the pack's curve, 1 - exp(-light × exposure), shows as `shown`, from 0 to just under 1.
float3 lightShowing(float3 shown, float exposure)
{
    return -log(1.0 - clamp(shown, 0.0, 0.985)) / max(exposure, 1e-6);
}

// A surface of colour `albedo` lit by `leaving`, the light it sends to the camera of the sun, the sky and the lamps,
// its colour in it already.
float3 banded(float3 albedo, float3 leaving, float3 emission)
{
    Options o = options();
    float exposure = frame()->exposure;
    // The light reaching it, as on a white surface.
    float3 reaching = leaving / max(albedo, 0.04);
    float brightest = max(reaching.r, max(reaching.g, reaching.b));
    float shown = 1.0 - exp(-brightest * exposure);
    // Steps even to the eye, in the square root of the brightness, so light falling off from a lamp shows as rings
    // of about the same width rather than crowding round the lamp.
    float steps = max(round(o.bands), 1.0);
    float stepped = floor(sqrt(shown) * steps + 0.5) / steps;
    stepped *= stepped;
    // The light's own colour, its brightest channel 1, a little richer in the dimmest steps.
    float3 hue = reaching / max(brightest, 1e-5);
    hue = pow(hue, lerp(1.3, 1.0, sqrt(stepped)));
    float3 shade = o.shade * (0.55 + 0.9 * albedo);
    float3 shown3 = shade + albedo * hue * stepped * (1.0 - shade);
    return lightShowing(shown3, exposure) + emission;
}
