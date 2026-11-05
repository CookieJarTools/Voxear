// Storybook: soft, painted light, made for the Storybook lighting look.
// Light turns gently to shade through a warm glow, the falling-off light
// takes a cool colour, highlights are broad and golden, light from behind
// catches the edges, and the colour wanders a little across the surface,
// as brushstrokes do.

struct Storybook
{
    [Property, Color] float3 tint = float3(1.0, 1.0, 1.0);
    [Property] Picture picture;
    [Property, Range(0, 1), Tooltip("How softly light turns to shade")] float softness = 0.6;
    [Property, Color, Tooltip("The colour of the light where it falls off towards shade")] float3 shade = float3(0.55, 0.55, 0.85);
    [Property, Color, Tooltip("The warm colour where light turns to shade")] float3 glow = float3(1.0, 0.5, 0.3);
    [Property, Range(0, 1)] float glowAmount = 0.35;
    [Property, Color] float3 highlight = float3(1.0, 0.85, 0.55);
    [Property, Range(0, 1), Tooltip("How broad and bright the golden highlights are. 0 for none")] float shine = 0.3;
    [Property, Range(0, 1), Tooltip("How much light coming from behind catches the edges")] float rim = 0.4;
    [Property, Range(0, 1), Tooltip("How much the colour wanders, as brushstrokes do")] float brushwork = 0.3;
    [Property, Range(0.05, 10), Tooltip("How big a brushstroke is, in metres")] float strokeSize = 0.6;
    [Property, Range(0, 2), Tooltip("How rich the colours are. 1 as they are")] float saturation = 1.15;
}

float hash(float3 p)
{
    p = frac(p * 0.1031);
    p += dot(p, p.zyx + 31.32);
    return frac((p.x + p.y) * p.z);
}

float noise(float3 p)
{
    float3 i = floor(p);
    float3 f = frac(p);
    float3 u = f * f * (3.0 - 2.0 * f);
    float bottom = lerp(lerp(hash(i), hash(i + float3(1, 0, 0)), u.x), lerp(hash(i + float3(0, 1, 0)), hash(i + float3(1, 1, 0)), u.x), u.y);
    float top = lerp(lerp(hash(i + float3(0, 0, 1)), hash(i + float3(1, 0, 1)), u.x), lerp(hash(i + float3(0, 1, 1)), hash(i + float3(1, 1, 1)), u.x), u.y);
    return lerp(bottom, top, u.z);
}

void surface(Storybook storybook, SurfaceInput input, inout SurfaceOutput surface)
{
    float3 color = surface.color * storybook.tint * storybook.picture.sample(input.uv).rgb;
    // Brushstrokes: colour and brightness wander, in strokes longer one way than the other, fixed to the Obj.
    float3 p = (input.position - input.objectPosition) / storybook.strokeSize;
    float a = noise(p * float3(1.0, 2.5, 1.0));
    float b = noise(p * 2.1 + 7.3);
    color *= 1.0 + storybook.brushwork * 0.6 * (float3(a, b, 0.5 * (a + b)) - 0.5);
    float grey = dot(color, float3(0.2126, 0.7152, 0.0722));
    surface.color = max(lerp(float3(grey), color, storybook.saturation), 0.0);
    surface.metallic = 0.0;
    surface.roughness = lerp(0.9, 0.5, storybook.shine);
}

float3 light(Storybook storybook, LightInput light, SurfaceOutput surface)
{
    float nl = dot(surface.normal, light.direction);
    // Lit flat over most of the side facing the light, falling off softly near the edge of shade.
    float lit = smoothstep(0.0, 0.1 + 0.9 * storybook.softness, nl);
    float3 diffuse = surface.color * lit * lerp(storybook.shade, float3(1.0), lit);
    // The warm band where light turns to shade, strongest halfway.
    float band = 4.0 * lit * (1.0 - lit);
    diffuse += storybook.glow * storybook.glowAmount * band * (0.4 + 0.6 * surface.color);

    float3 halfway = normalize(light.direction + light.view);
    float broad = pow(saturate(dot(surface.normal, halfway)), lerp(6.0, 48.0, storybook.shine));
    float3 shine = storybook.highlight * broad * storybook.shine * lit;

    float edge = pow(1.0 - saturate(dot(surface.normal, light.view)), 3.0);
    float behind = saturate(dot(-light.view, light.direction) * 0.7 + 0.3);
    float3 rim = storybook.highlight * storybook.rim * edge * behind * saturate(nl + 0.3);
    return light.color * (diffuse / PI + shine * 0.5 + rim * 0.4);
}
