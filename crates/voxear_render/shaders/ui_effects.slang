// The UI's built-in effects (`shader: shimmer`, `stripes`, `scanlines` and
// `static`), always compiled in, for the editor's UI and games' alike. Each
// reads its properties from the frame's UI values, in the order voxear_ui's
// UiShaders::default lists them, colours linear and premultiplied.

// A number from 0 to 1 that seems random, for a point.
float uiHash(float2 p)
{
    p = frac(p * float2(123.34, 456.21));
    p += dot(p, p + 45.32);
    return frac(p.x * p.y);
}

// `a` over `b`, both premultiplied.
float4 uiOver(float4 a, float4 b)
{
    return a + b * (1.0 - a.a);
}

// A band of light sweeping across, leaning a little: speed (crossings a
// second), width (of the box), shine (its colour).
float4 uiShimmer(UiInput input, float4* values)
{
    float speed = values[0].x;
    float width = max(values[1].x, 0.01);
    float lean = input.uv.y * 0.3 * input.size.y / max(input.size.x, 1.0);
    float at = frac(input.time * speed) * (1.3 + 2.0 * width) - width;
    float band = saturate(1.0 - abs(input.uv.x + lean - at) / width);
    return uiOver(values[2] * band * band * (3.0 - 2.0 * band), input.background);
}

// Stripes moving across: stripe (their colour), spacing (pixels from one to
// the next), speed (pixels a second), angle (degrees).
float4 uiStripes(UiInput input, float4* values)
{
    float spacing = max(values[1].x, 1.0);
    float angle = radians(values[3].x);
    float along = dot(input.position, float2(cos(angle), sin(angle))) - input.time * values[2].x;
    float place = frac(along / spacing);
    // Half of each spacing is stripe, its edges a pixel soft.
    float soft = max(fwidth(along) / spacing, 1e-4);
    float on = saturate((0.25 - abs(place - 0.5)) / soft + 0.5);
    return uiOver(values[0] * on, input.background);
}

// Dark lines across and a faint flicker, as an old screen has: spacing
// (pixels between lines), strength (how dark), flicker (how much).
float4 uiScanlines(UiInput input, float4* values)
{
    float spacing = max(values[0].x, 1.0);
    // Lines closer than the pixels they land on, as on a panel far off,
    // would alias into bands, so they fade to their average first.
    float across = spacing / max(fwidth(input.position.y), 1e-4);
    float contrast = saturate((across - 1.5) / 1.5);
    float line = 0.5 + 0.5 * contrast * cos(input.position.y / spacing * 6.2831853);
    float flicker = 1.0 - values[2].x * uiHash(float2(floor(input.time * 24.0), 7.0));
    float dim = (1.0 - saturate(values[1].x) * line) * flicker;
    return float4(input.background.rgb * dim, input.background.a);
}

// Grain that changes as it plays: amount (how strong), speed (changes a
// second).
float4 uiStatic(UiInput input, float4* values)
{
    float frame = floor(input.time * max(values[1].x, 0.0));
    float grain = uiHash(floor(input.position * input.scale) + frame * float2(17.0, 31.0)) - 0.5;
    float4 c = input.background;
    return float4(clamp(c.rgb + grain * values[0].x * c.a, 0.0, c.a), c.a);
}

// The built-in effect `effect`, its values from `first` on.
float4 builtinUi(uint effect, uint first, UiInput input)
{
    float4* values = buffers()->uiValues + first;
    switch (effect)
    {
    case 0:
        return uiShimmer(input, values);
    case 1:
        return uiStripes(input, values);
    case 2:
        return uiScanlines(input, values);
    case 3:
        return uiStatic(input, values);
    default:
        return input.background;
    }
}
