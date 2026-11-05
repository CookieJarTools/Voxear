// Toon: light in flat steps, as a cartoon is painted, with a coloured shade
// rather than a dark one, a crisp shine and a rim of light round the edges.

struct Toon
{
    [Property, Color] float3 tint = float3(1.0, 1.0, 1.0);
    [Property] Picture picture;
    [Property, Range(2, 6), Tooltip("How many steps of light, from shade to lit")] float steps = 2.0;
    [Property, Range(0, 1), Tooltip("How soft the edge between two steps is")] float softness = 0.15;
    [Property, Color, Tooltip("The colour of the shaded step, over the surface's own")] float3 shade = float3(0.45, 0.4, 0.6);
    [Property, Range(0, 1), Tooltip("How big the shiny spot is. 0 for none")] float shine = 0.3;
    [Property, Color] float3 rim = float3(1.0, 1.0, 1.0);
    [Property, Range(0, 1), Tooltip("How far in from the edges the rim of light reaches. 0 for none")] float rimWidth = 0.2;
    [Property, Range(0, 1), Tooltip("Darkens edges that turn away, as an ink line. Best on rounded shapes")] float ink = 0.0;
}

void surface(Toon toon, SurfaceInput input, inout SurfaceOutput surface)
{
    surface.color *= toon.tint * toon.picture.sample(input.uv).rgb;
    // Matt and not metal, so reflections do not smooth the steps away.
    surface.metallic = 0.0;
    surface.roughness = 1.0;
    if (toon.ink > 0.0)
    {
        float facing = saturate(dot(surface.normal, input.view));
        float edge = toon.ink * 0.5;
        surface.color *= smoothstep(edge - 0.03, edge + 0.03, facing);
    }
}

float3 light(Toon toon, LightInput light, SurfaceOutput surface)
{
    float nl = saturate(dot(surface.normal, light.direction));
    // Steps spread over the lit side, more of it lit than not, each rising softly to the next.
    float x = sqrt(nl) * toon.steps;
    float soft = max(toon.softness, 0.02) * 0.5;
    float stepped = floor(x) + smoothstep(1.0 - soft, 1.0, frac(x));
    float level = saturate(stepped / max(toon.steps - 1.0, 1.0));
    float3 lit = surface.color * lerp(toon.shade, float3(1.0), level) / PI;

    float3 halfway = normalize(light.direction + light.view);
    float size = 1.0 - toon.shine * toon.shine * 0.08;
    float spot = toon.shine > 0.0 && nl > 0.0 ? smoothstep(size - 0.003, size + 0.003, dot(surface.normal, halfway)) : 0.0;

    float edge = 1.0 - saturate(dot(surface.normal, light.view));
    float rim = toon.rimWidth > 0.0 ? smoothstep(1.0 - toon.rimWidth - 0.02, 1.0 - toon.rimWidth + 0.02, edge) * level : 0.0;
    return light.color * (lit + spot + toon.rim * rim * 0.3);
}
