// Radiant's air near the camera: the way from the camera to each pixel's surface is walked, and at each step the air
// glows with what reaches it. The sun reaches it where the sun's shadow says nothing stands in its way, so shafts of
// light come down through the gaps in tree crowns and past models and characters; and, in a world of blocks, where a
// walk from there towards the sun through the light kept round the camera meets no solid block before the open sky, so
// a cave's roof keeps it dark however far the shadow maps reach. A little of the sky's and the torches' light glows
// too, as much as the block there has. In a cave the air is dustier, so sun through a hole in its roof shows as a beam
// and the rest stays dark. Written at half size: the light the air adds in rgb, and in alpha how much of what is
// behind still shows.

#include "window.slang"

static const int STEPS = 20;
static const float REACH = 64.0;
// How much haze open air holds: enough that sun between shadows shows as shafts, not so much that it greys the view.
static const float OPEN_AIR = 0.0012;

float4 run(PassInput input)
{
    Options o = options();
    PackFrame* f = frame();
    if (!o.sunbeams || o.strength <= 0.0)
    {
        return float4(0.0, 0.0, 0.0, 1.0);
    }
    bool blocks = hasBlockLight();
    float size = blocks ? f->lightWindow->blockSize : 1.0;
    float3 way = viewRay(input.uv);
    float reach = min(isSky(input.uv) ? REACH : sceneDistance(input.uv), REACH);
    float step = reach / float(STEPS);
    float noise = frameNoise(input.pixel);
    float t = step * noise;

    float3 toSun = f->sunDirection;
    // A high sun's beam nearly white, a low one's warm.
    float3 sun = sunLight() * (1.0 - f->night * 0.85) * (1.0 - f->cloud * 0.7);
    float high = smoothstep(0.3, 0.8, toSun.y);
    sun = lerp(sun, luminance(sun) * float3(0.95, 0.98, 1.05), 0.6 * high);
    // Light scattered towards the camera: a forward glow looking towards the sun, and some to every side, so a beam
    // shows across the view too.
    float cosine = dot(way, toSun);
    float g = 0.65;
    float forward = (1.0 - g * g) / pow(max(1.0 + g * g - 2.0 * g * cosine, 1e-4), 1.5);
    float phase = (0.6 + 0.4 * forward) / (4.0 * PACK_PI);
    float3 sky = skyAround();
    float3 torch = blockGlow();

    float3 added = 0.0;
    float through = 1.0;
    for (int i = 0; i < STEPS; i++)
    {
        float3 at = f->cameraPosition + way * t;
        float2 light = lightAround(at);
        // Open air holds a little haze; a cave's air more dust, the more so the less sky light it has, and under a
        // hole in its roof as much: there the blocks round about are under the roof, out of the open sky.
        float cave = 0.0;
        if (blocks)
        {
            float walled = 0.0;
            for (uint side = 0; side < 4; side++)
            {
                float3 off = float3(side == 0 ? 5.0 : side == 1 ? -5.0 : 0.0, 0.0, side == 2 ? 5.0 : side == 3 ? -5.0 : 0.0);
                walled += (windowCell(at + off * size) >> 4) < 15 ? 0.25 : 0.0;
            }
            cave = max(1.0 - light.x, walled * walled);
        }
        float density = (OPEN_AIR * o.haze + 0.03 * cave * cave * o.cave_darkness) * o.beam_strength;
        // The sun's shadow anywhere it is drawn; under cover, the walk through the blocks as well.
        float lit = sunVisibility(at);
        if (blocks && cave > 0.0 && lit > 0.0)
        {
            lit *= sunReaches(at, toSun, size, frac(noise + float(i) * 0.618));
        }
        float3 here = sun * lit * phase * 4.0 + sky * light.x * light.x * 0.08 + torch * light.y * light.y * light.y * 0.15;
        float absorbed = exp(-density * step);
        added += through * here * (1.0 - absorbed);
        through *= absorbed;
        t += step;
    }
    return float4(added * o.strength, lerp(1.0, through, saturate(o.strength)));
}
