// Hologram: a figure of light, brighter at its edges than in the middle,
// with lines scrolling up it and a flicker. Unlit, the same in light and
// shade. Turn on the material's Glass to see through it, as a projected
// image or a force field: its light then adds to what is behind.

[Unlit]
struct Hologram
{
    [Property, Color] float3 tint = float3(0.2, 0.75, 1.0);
    [Property, Range(0, 20)] float brightness = 1.2;
    [Property, Range(0, 1), Tooltip("How much brighter the edges are than the middle")] float edges = 0.7;
    [Property, Range(0, 100), Tooltip("How many lines a metre. 0 for none")] float lines = 30.0;
    [Property, Range(-5, 5), Tooltip("How fast the lines move up it, in metres a second")] float scroll = 0.3;
    [Property, Range(0, 1)] float flicker = 0.15;
    [Property, Tooltip("A pattern over it, as hexagons for a force field")] Picture pattern;
    [Property, Range(0.1, 10), Tooltip("How many metres the pattern covers")] float patternSize = 1.0;
}

float hash(float n)
{
    return frac(sin(n * 12.9898) * 43758.5453);
}

void surface(Hologram hologram, SurfaceInput input, inout SurfaceOutput surface)
{
    float facing = abs(dot(input.normal, input.view));
    float edge = pow(1.0 - facing, 2.0);
    float body = lerp(1.0, 0.2 + 1.6 * edge, hologram.edges);

    float shown = 1.0;
    if (hologram.lines > 0.0)
    {
        float band = frac((input.levelPosition.y - input.time * hologram.scroll) * hologram.lines);
        shown = lerp(1.0, 0.35, smoothstep(0.75, 0.85, band) * (1.0 - smoothstep(0.92, 1.0, band)));
    }
    // Now and then a sudden dip, over a constant shimmer.
    float dip = step(0.88, hash(floor(input.time * 14.0))) * 0.6;
    float shimmer = 0.15 * (0.5 + 0.5 * sin(input.time * 31.0));
    float flicker = 1.0 - hologram.flicker * (dip + shimmer);

    float3 p = (input.position - input.objectPosition) / hologram.patternSize;
    float3 weights = pow(abs(input.normal), 4.0);
    weights /= max(weights.x + weights.y + weights.z, 1e-4);
    float pattern = dot(float3(hologram.pattern.sample(p.zy).r, hologram.pattern.sample(p.xz).r, hologram.pattern.sample(p.xy).r), weights);

    surface.color = hologram.tint * hologram.brightness * body * shown * flicker * pattern;
    surface.emission = 0.0;
}
