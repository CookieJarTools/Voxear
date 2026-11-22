// Radiant's light on water: the sun's path on it warm; where the sun reaches into it, the water lit through a clear
// blue, as a pool in a cave glows where a beam falls on it; in a cave, the sky it mirrors as dark as the cave; and its
// foam as the engine has it.

#include "window.slang"

// A web of bright lines as rippled water focuses the sun, moving with the time; about 0.5 on average.
float caustics(float3 position, float time)
{
    float2 p = position.xz * 1.1;
    float sum = 0.0;
    for (int i = 0; i < 3; i++)
    {
        p += float2(sin(p.y * 1.3 + time * 0.8 + float(i)), cos(p.x * 1.1 - time * 0.6)) * 0.6;
        sum += 0.12 / (0.12 + abs(sin(p.x) * cos(p.y)));
    }
    return saturate(sum / 3.0 - 0.1);
}

float3 run(WaterSurface surface, WaterLighting light)
{
    Options o = options();
    PackFrame* f = frame();
    float s = o.strength;
    float low = 1.0 - smoothstep(0.08, 0.7, light.sunDirection.y);
    float warmth = o.warmth * lerp(0.5, 1.0, low) * (1.0 - f->night * 0.7) * min(s, 1.5);
    float3 gold = lerp(float3(1.0), float3(1.3, 1.0, 0.65), warmth);
    float deep = pow(saturate(surface.skyLight), 1.6 * o.cave_darkness * min(s, 1.0));
    float toward = pow(saturate(dot(reflect(-surface.view, surface.normal), light.sunDirection)), 6.0);
    float3 mirrored = light.reflection * lerp(float3(1.0), gold, toward) * lerp(1.0, deep, saturate(s));
    // The sun lighting the water through: a clear blue, in the bright moving web the ripples focus it into.
    float lit = light.shadow;
    // Under cover, only where a walk towards the sun finds the open sky between the blocks: the water's own shadow
    // does not see a cave's roof.
    PackLightWindow* window = f->lightWindow;
    float shut = 0.0;
    if (window != nullptr && window->found != 0)
    {
        shut = (1.0 - smoothstep(7.0, 14.0, float(windowCell(f->cameraPosition) >> 4))) * saturate(s);
        if (shut > 0.0)
        {
            float reach = sunReaches(surface.position + float3(0.0, 0.2, 0.0) * window->blockSize, light.sunDirection,
                window->blockSize);
            lit = lerp(lit, min(lit, reach), shut);
        }
    }
    lit *= 1.0 - f->night * 0.8;
    // Strongest where it is dark round about, in a cave; the open water keeps most of the engine's look.
    float3 inside = light.sunLight * lit * float3(0.45, 0.7, 0.9) * 0.3 * s * (0.45 + 0.9 * caustics(surface.position, f->time))
        * (1.0 - exp(-max(surface.depth, 0.3) * 1.5)) * lerp(0.12, 1.0, shut);
    // In a cave the water's own glow is as dark as the cave round it, but where the sun is in it.
    float dim = lerp(1.0, max(deep * deep, lit), shut);
    float3 under = (light.behind * light.through + light.scatter) * dim + inside;
    float3 clear = (1.0 - light.fresnel) * under + light.fresnel * mirrored + light.glints * gold;
    // What the engine lays over the clear water, its foam and white water, laid over this as it is.
    return clear + (light.engine - light.clear);
}
