// Radiant's light on block faces: a low sun warm and the shade under the sky a cool blue-grey; deeper in a cave, the
// further from the sky, darker than the engine has it, so light from an opening reaches only a little way in;
// torchlight bright close by and falling off quickly, brightest on what faces the torch; and leaves lit through from
// behind against the sun.

#include "window.slang"

float3 run(BlockSurface surface, BlockLighting light)
{
    Options o = options();
    PackFrame* f = frame();
    float s = o.strength;
    float low = 1.0 - smoothstep(0.08, 0.7, light.sunDirection.y);
    float warmth = o.warmth * lerp(0.5, 1.0, low) * (1.0 - f->night * 0.7) * min(s, 1.5);
    float3 diffuse = surface.color * (1.0 - surface.metallic);

    // A low sun's light kept strong, as the evening's light reads: the engine's thins it towards the horizon.
    float3 sun = light.sunTerm * lerp(float3(1.0), float3(1.3, 1.02, 0.7), warmth) * (1.0 + 0.7 * low * min(s, 1.5));
    float3 sky = light.sky * lerp(float3(1.0), float3(0.8, 0.92, 1.25), warmth);
    // The sky's light falls off faster into a cave than a step a block: at half sky light, a third of the engine's.
    float deep = pow(saturate(surface.skyLight), 1.6 * o.cave_darkness * min(s, 1.0));
    // Faces turned down see the ground and the cave round them, not the sky: a cave's roof stays dark beside an
    // opening, where the sky light spreading round its edge would light it.
    sky *= deep * (1.0 - 0.6 * o.cave_darkness * saturate(s) * saturate(-surface.faceNormal.y));

    // Torchlight: the engine's, falling off quicker the further from the torch, brighter on what faces it.
    float3 glow = light.glow * lerp(float3(1.0), float3(1.12, 0.94, 0.78), warmth);
    float near = saturate(surface.blockLight);
    glow *= o.torchlight * lerp(1.0, near * near * near * 2.2, saturate(s));
    // A torch's yellow close by, greyer further off, where it is the little light left bouncing round the stone.
    float3 flame = lerp(float3(1.0, 0.8, 0.5), float3(0.9, 0.93, 1.0), saturate((1.0 - near) * 2.0));
    glow = lerp(glow, luminance(glow) * flame * 1.1, saturate(s));
    if (near > 0.01)
    {
        float3 way = torchWay(surface.position, surface.faceNormal);
        float strength = length(way);
        if (strength > 1e-3)
        {
            float facing = dot(surface.normal, way / strength);
            glow *= lerp(1.0, 0.45 + 0.9 * saturate(facing + 0.25), saturate(s * strength * 6.0));
        }
    }

    // Under cover, the sun where a walk from the face towards it finds the open sky between the blocks: through a hole
    // in a cave's roof, down a shaft, in at a cave's mouth, as a patch on the floor.
    PackLightWindow* window = f->lightWindow;
    float facing = saturate(dot(surface.normal, light.sunDirection));
    if (window != nullptr && window->found != 0 && facing > 0.0 && light.shadow < 0.99 && o.sunbeams)
    {
        float shut = 1.0 - smoothstep(7.0, 14.0, float(windowCell(f->cameraPosition) >> 4));
        if (shut > 0.0)
        {
            float size = window->blockSize;
            float reach = sunReaches(surface.position + surface.faceNormal * (0.3 * size), light.sunDirection, size);
            float more = max(reach - light.shadow, 0.0) * shut * saturate(s);
            sun += light.sunLight * diffuse * facing * more * lerp(float3(1.0), float3(1.3, 1.02, 0.7), warmth) * (1.4 / PACK_PI);
        }
    }

    float3 color = sun + (sky + glow) * diffuse * light.occlusion + light.reflectionTerm + light.lamps + light.emission;
    // Metal and ore catch the torchlight: the light of the blocks round it, in its own colour.
    color += surface.color * surface.metallic * glow * light.occlusion * 0.45;
    if ((surface.kind & (BLOCK_LEAVES | BLOCK_SHEET | BLOCK_CUT_OUT)) != 0)
    {
        // Seen against the sun, leaves and plants glow with the light through them, where the sun reaches them.
        float against = pow(saturate(dot(-surface.view, light.sunDirection)), 4.0);
        color += diffuse * light.sunLight * light.shadow * against * 0.35 * o.foliage_glow * s * (1.0 - f->night * 0.8);
    }
    return color;
}
