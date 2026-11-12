// What particles.slang and the sea's spray in water.slang share: working
// out when a particle set out, its random numbers, how it is lit, placing
// its quad and drawing its pixels.

#pragma once
#include "shading.slang"
#include "volume.slang"
#include "particle_motion.slang"

struct ParticleOut
{
    float4 position : SV_Position;
    // Where in its quad, from -1 to 1 each way.
    float2 corner : CORNER;
    nointerpolation float4 color : COLOR;
    // How far from the camera along its view, for testing against the scene's depth.
    nointerpolation float depth : DEPTH;
    nointerpolation float softness : SOFTNESS;
    nointerpolation uint additive : ADDITIVE;
    // The texture it is drawn with, or NO_TEXTURE for a soft round dot; and which of the texture's pictures: where
    // its corner is in the texture, then how much of the texture it takes.
    nointerpolation uint texture : TEXTURE;
    nointerpolation float4 tile : TILE;
    // Playing through its pictures: the next one, and how far toward it, from 0 to 1.
    nointerpolation float4 tileNext : TILENEXT;
    nointerpolation float blend : BLEND;
    // Lit from six sides: its second map, or NO_TEXTURE when it is not; the light that falls on it from its right,
    // its left, above, below, in front and behind; and the fog's light over it, which its colour then leaves out.
    nointerpolation uint lightMap : LIGHTMAP;
    nointerpolation float3 lightRight : LIGHTRIGHT;
    nointerpolation float3 lightLeft : LIGHTLEFT;
    nointerpolation float3 lightTop : LIGHTTOP;
    nointerpolation float3 lightBottom : LIGHTBOTTOM;
    nointerpolation float3 lightFront : LIGHTFRONT;
    nointerpolation float3 lightBack : LIGHTBACK;
    nointerpolation float3 haze : HAZE;
    // How much of its opacity shows here, from 0 to 1: all of it but along a trail, which fades from point to point.
    float fade : FADE;
};

// When slot `slot` of `system` last set out a particle, up to the system's
// time now, and which time round that was: false when it has none, as
// before its first or once the system has stopped and the last has set out.
bool particleSpawn(ParticleSystem system, uint slot, out float spawn, out uint round)
{
    float t = system.time;
    bool looping = (system.flags & PARTICLES_LOOPING) != 0;
    spawn = 0.0;
    round = 0;
    if (t < 0.0)
    {
        return false;
    }
    // The burst, as each round starts.
    if (slot < system.burst)
    {
        float last = looping ? floor(min(t, system.stopped) / system.duration) : 0.0;
        spawn = last * system.duration;
        round = uint(last);
        return spawn <= system.stopped;
    }
    // The steady stream: this slot's turn comes every `period`, starting `first` in.
    uint stream = system.slots - system.burst;
    if (system.rate <= 0.0 || stream == 0)
    {
        return false;
    }
    float period = float(stream) / system.rate;
    float first = float(slot - system.burst) / system.rate;
    float until = min(looping ? 1e30 : system.duration, system.stopped);
    if (t < first || first >= until)
    {
        return false;
    }
    float last = floor((t - first) / period);
    last = min(last, ceil((until - first) / period) - 1.0);
    spawn = first + last * period;
    round = uint(last) + 1;
    return true;
}

// The light falling on a puff of smoke at `world`: the sky's and the GI's
// from every way, as fog takes it, since smoke scatters light that comes
// from anywhere; and the sun's and the lamps', each as a ball takes it, lit
// on one side.
float3 particleLight(Frame* frame, float3 world, float4 clip)
{
    bool atmosphere = frame->lighting.sky == SKY_ATMOSPHERE;
    float3 sunLight = atmosphere ? sunThroughAtmosphere(frame) : frame->sunColor;
    float3 toCamera = normalizeOr(frame->cameraPosition - world, float3(0.0, 1.0, 0.0));
    float3 ambient = atmosphere ? atmosphereAmbient(frame, 0.5, sunLight)
        : lerp(frame->groundColor, lerp(frame->skyHorizon, frame->skyZenith, 0.5), 0.5) * 0.35 * frame->ambient;
    if (frame->ddgi.enabled != 0)
    {
        ambient = ddgiLightAround(frame, world, toCamera, ambient);
    }
    float3 lit = sunLight * sunShadow(frame, world, -frame->sunDirection);
    float2 pixel = (clip.xy / clip.w * 0.5 + 0.5) * float2(frame->viewport);
    uint base = clusterAt(frame, pixel, max(clip.w, frame->near)) * (frame->clusterMaxLights + 1);
    for (uint i = 0; i < buffers()->clusters[base]; i++)
    {
        Light lamp = buffers()->lights[buffers()->clusters[base + 1 + i]];
        lit += localLightInput(lamp, world, normalizeOr(lamp.position - world, toCamera)).color;
    }
    // A ball lit from one side takes half its light, spread over it.
    return ambient + lit * 0.5 / PI;
}

// The light falling on a puff at `world` from each of six sides, for one lit by its maps (PARTICLES_SIX_WAY): from
// along `right`, against it, along `up`, against it, along `front` (toward the camera) and from behind. The sky's
// and the GI's light comes from every side alike; the sun's and each lamp's from its own, on the sides that face it.
void particleLightSix(Frame* frame, float3 world, float4 clip, float3 right, float3 up, float3 front, out float3 six[6])
{
    bool atmosphere = frame->lighting.sky == SKY_ATMOSPHERE;
    float3 sunLight = atmosphere ? sunThroughAtmosphere(frame) : frame->sunColor;
    float3 ambient = atmosphere ? atmosphereAmbient(frame, 0.5, sunLight)
        : lerp(frame->groundColor, lerp(frame->skyHorizon, frame->skyZenith, 0.5), 0.5) * 0.35 * frame->ambient;
    if (frame->ddgi.enabled != 0)
    {
        ambient = ddgiLightAround(frame, world, front, ambient);
    }
    float3 sides[6] = { right, -right, up, -up, front, -front };
    for (uint side = 0; side < 6; side++)
    {
        six[side] = ambient;
    }
    float3 toSun = -frame->sunDirection;
    float3 sun = sunLight * sunShadow(frame, world, toSun) / PI;
    for (uint side = 0; side < 6; side++)
    {
        six[side] += sun * saturate(dot(toSun, sides[side]));
    }
    float2 pixel = (clip.xy / clip.w * 0.5 + 0.5) * float2(frame->viewport);
    uint base = clusterAt(frame, pixel, max(clip.w, frame->near)) * (frame->clusterMaxLights + 1);
    for (uint i = 0; i < buffers()->clusters[base]; i++)
    {
        Light lamp = buffers()->lights[buffers()->clusters[base + 1 + i]];
        float3 toLamp = normalizeOr(lamp.position - world, front);
        float3 lit = localLightInput(lamp, world, toLamp).color / PI;
        for (uint side = 0; side < 6; side++)
        {
            six[side] += lit * saturate(dot(toLamp, sides[side]));
        }
    }
}

// The corners of a particle's quad, as its six vertices take them.
static const float2 PARTICLE_CORNERS[6] = { float2(-1.0, -1.0), float2(1.0, -1.0), float2(1.0, 1.0), float2(-1.0, -1.0), float2(1.0, 1.0), float2(-1.0, 1.0) };

// A vertex of a slot with no particle now: out of view, drawing nothing.
ParticleOut particleNone(ParticleSystem system, float2 corner)
{
    ParticleOut output;
    output.corner = corner;
    output.color = 0.0;
    output.depth = 0.0;
    output.softness = system.softness;
    output.additive = 0;
    output.texture = NO_TEXTURE;
    output.tile = float4(0.0, 0.0, 1.0, 1.0);
    output.tileNext = float4(0.0, 0.0, 1.0, 1.0);
    output.blend = 0.0;
    output.lightMap = NO_TEXTURE;
    output.lightRight = 0.0;
    output.lightLeft = 0.0;
    output.lightTop = 0.0;
    output.lightBottom = 0.0;
    output.lightFront = 0.0;
    output.lightBack = 0.0;
    output.haze = 0.0;
    output.fade = 1.0;
    output.position = float4(0.0, 0.0, -1.0, 1.0);
    return output;
}

// The air between the camera and a particle at `world`, `clip` on screen:
// in alpha how much of the particle shows through it, and the light the
// haze and the fog add over it. The fog's volume, when the volumetric fog
// is on, is push.textures.y; otherwise the height fog, as the post passes
// lay it over the scene behind.
float4 particleFog(Frame* frame, float3 world, float4 clip)
{
    float3 ray = world - frame->cameraPosition;
    float distance = max(length(ray), 1e-4);
    float2 uv = saturate(clip.xy / clip.w * 0.5 + 0.5);
    float3 added = 0.0;
    float through = 1.0;
    if (frame->lighting.sky == SKY_ATMOSPHERE)
    {
        float4 haze = aerialPerspectiveAt(frame, uv, distance);
        added = haze.rgb * skyLook(frame);
        through = haze.a;
    }
    if (push.textures.y != NO_TEXTURE)
    {
        float far = frame->post.fogVolumeFar;
        float4 fog = volumeAt(push.textures.y, uv, min(clip.w, far), far);
        return float4(added * fog.a + fog.rgb, through * fog.a);
    }
    if (frame->post.fogDensity <= 0.0)
    {
        return float4(added, through);
    }
    float falloff = max(frame->post.fogHeightFalloff, 1e-4);
    float density = frame->post.fogDensity * exp(-falloff * (frame->cameraPosition.y - frame->post.fogBaseHeight));
    float amount = 1.0 - heightFogThrough(density, falloff, ray);
    float glow = pow(saturate(dot(ray / distance, -frame->sunDirection)), 8.0) * frame->post.fogSunGlow;
    float3 color = (fogColourToward(frame, ray / distance) + frame->sunColor * glow * 0.15) * fogOpenness(frame, world);
    return float4(added * (1.0 - amount) + color * amount, through * (1.0 - amount));
}

// How wide a particle that has lived `aged` of its life is now, in metres, by the first of its random numbers `h`,
// which it leaves as they were.
float particleWide(ParticleSystem system, float aged, uint h)
{
    float grown = system.curveCounts.x > 0 ? curveAt(system.sizeCurve, system.curveCounts.x, aged) : lerp(1.0, system.sizeEnd, aged);
    return lerp(system.size.x, system.size.y, particleRandom(h)) * grown;
}

// Places the quad of a particle at `world`, going at `velocity`, that has
// lived `aged` of its life, `age` seconds, of `system`, at `corner` of its
// quad, with its random numbers `h`; and how bright and opaque it shows,
// `fade` of it. A system that stretches its particles draws each along its
// way across the view, longer the faster it goes, rather than turned.
ParticleOut particlePlaceGoing(Frame* frame, ParticleSystem system, float3 world, float3 velocity, float age, float aged, float2 corner, inout uint h, float fade)
{
    ParticleOut output;
    output.corner = corner;
    output.softness = system.softness;
    output.additive = (system.flags & PARTICLES_ADDITIVE) != 0 ? 1 : 0;
    float size = particleWide(system, aged, h);
    particleRandom(h);
    float turned = particleRandom(h) * 2.0 * PI + system.spin * age * (particleRandom(h) < 0.5 ? -1.0 : 1.0);
    float2 spun = float2(corner.x * cos(turned) - corner.y * sin(turned), corner.x * sin(turned) + corner.y * cos(turned));
    float3 forward = frame->cameraForward;
    float3 right = normalizeOr(cross(float3(0.0, 1.0, 0.0), forward), float3(1.0, 0.0, 0.0));
    float3 up = cross(forward, right);
    float3 at = world + (right * spun.x + up * spun.y) * size * 0.5;
    // Which way its picture's right and top are in the world, as it is turned.
    float3 pictureRight = right * cos(turned) + up * sin(turned);
    float3 pictureUp = up * cos(turned) - right * sin(turned);
    float3 across = velocity - forward * dot(velocity, forward);
    float going = length(across);
    if (system.stretch > 0.0 && going > 1e-3)
    {
        // Along its way as the view sees it, and as wide as it is across it.
        float3 along = across / going;
        float3 side = cross(forward, along);
        at = world + side * corner.x * size * 0.5 + along * corner.y * (size * 0.5 + going * system.stretch * 0.5);
        pictureRight = side;
        pictureUp = along;
    }
    float4 clip = transform(frame->viewProj, float4(at, 1.0));

    // Which of its texture's pictures: one for good, picked by its own number, or each in turn over its life.
    // Playing through them, each fades into the next, so a few pictures play smoothly.
    output.texture = system.texture;
    uint2 tiles = max(system.tiles, 1);
    uint pictures = tiles.x * tiles.y;
    bool playing = (system.flags & PARTICLES_ANIMATE) != 0;
    float played = aged * float(pictures);
    uint picture = playing ? min(uint(played), pictures - 1) : particleHash(h) % pictures;
    uint next = min(picture + 1, pictures - 1);
    float2 share = 1.0 / float2(tiles);
    output.tile = float4(float2(picture % tiles.x, picture / tiles.x) * share, share);
    output.tileNext = float4(float2(next % tiles.x, next / tiles.x) * share, share);
    output.blend = playing && next != picture ? saturate(played - float(picture)) : 0.0;

    float4 color = lerp(system.color, system.colorEnd, aged);
    if (system.curveCounts.w > 0)
    {
        color.rgb = gradientAt(system.gradient, system.curveCounts.w, aged);
    }
    if (system.curveCounts.y > 0)
    {
        color.a = system.color.a * curveAt(system.opacityCurve, system.curveCounts.y, aged);
    }
    float4 centre = transform(frame->viewProj, float4(world, 1.0));
    bool lit = (system.flags & PARTICLES_LIT) != 0;
    bool sixWay = lit && (system.flags & PARTICLES_SIX_WAY) != 0;
    // Seen through the air to it: a particle that covers what is behind takes on the fog's light too, while one
    // that adds its light only dims, as the fog's light is behind it already.
    float4 air = particleFog(frame, world, centre);
    float3 hazed = output.additive != 0 ? 0.0 : air.rgb;
    output.lightMap = NO_TEXTURE;
    output.lightRight = 0.0;
    output.lightLeft = 0.0;
    output.lightTop = 0.0;
    output.lightBottom = 0.0;
    output.lightFront = 0.0;
    output.lightBack = 0.0;
    output.haze = 0.0;
    float3 shown;
    if (sixWay)
    {
        // Lit a pixel at a time from its maps: the light from each side goes with it, and its colour without any.
        float3 six[6];
        particleLightSix(frame, world, centre, pictureRight, pictureUp, -forward, six);
        output.lightMap = system.lightMap;
        output.lightRight = six[0];
        output.lightLeft = six[1];
        output.lightTop = six[2];
        output.lightBottom = six[3];
        output.lightFront = six[4];
        output.lightBack = six[5];
        output.haze = hazed;
        shown = color.rgb * system.intensity * air.a;
    }
    else
    {
        float3 light = lit ? particleLight(frame, world, centre) : 1.0;
        shown = color.rgb * light * system.intensity * air.a + hazed;
    }
    output.color = float4(shown, saturate(color.a * fade));
    output.fade = 1.0;
    output.depth = clip.w;
    output.position = clip;
    return output;
}


// The same, for a particle that is not going anywhere the caller knows of.
ParticleOut particlePlace(Frame* frame, ParticleSystem system, float3 world, float age, float aged, float2 corner, inout uint h, float fade)
{
    return particlePlaceGoing(frame, system, world, 0.0, age, aged, corner, h, fade);
}

// A particle's pixel: its texture's picture, or without one a soft round
// dot; faded before what it meets, and hidden behind it (the scene's depth,
// push.textures.x). Drawn at half size, push.textures.z is 2: how many of the
// depth's pixels each of its own spans.
[shader("fragment")]
float4 particleFragment(ParticleOut input) : SV_Target0
{
    Frame* frame = push.frame;
    float edge = dot(input.corner, input.corner);
    float3 tint = 1.0;
    float shape = (1.0 - edge) * (1.0 - edge);
    float3 haze = 0.0;
    if (input.texture != NO_TEXTURE)
    {
        float2 within = float2(input.corner.x, -input.corner.y) * 0.5 + 0.5;
        float4 picture = sampleTexture(input.texture, input.tile.xy + within * input.tile.zw);
        if (input.blend > 0.0)
        {
            picture = lerp(picture, sampleTexture(input.texture, input.tileNext.xy + within * input.tileNext.zw), input.blend);
        }
        tint = picture.rgb;
        shape = picture.a;
        if (input.lightMap != NO_TEXTURE)
        {
            // Lit from six sides: the picture is how bright from the right, the top and behind, its second map
            // from the left, below and in front.
            float4 other = sampleTexture(input.lightMap, input.tile.xy + within * input.tile.zw);
            if (input.blend > 0.0)
            {
                other = lerp(other, sampleTexture(input.lightMap, input.tileNext.xy + within * input.tileNext.zw), input.blend);
            }
            tint = picture.r * input.lightRight + other.r * input.lightLeft + picture.g * input.lightTop
                + other.g * input.lightBottom + other.b * input.lightFront + picture.b * input.lightBack;
            haze = input.haze;
        }
    }
    else if (edge >= 1.0)
    {
        discard;
    }
    int2 under = int2(input.position.xy * float(max(push.textures.z, 1u)));
    float behind = textures[NonUniformResourceIndex(push.textures.x)].Load(int3(under, 0)).r;
    float gap = (behind > 0.0 ? frame->near / behind : 1e4) - input.depth;
    if (gap <= 0.0)
    {
        discard;
    }
    float soft = input.softness > 0.0 ? saturate(gap / input.softness) : 1.0;
    float alpha = input.color.a * input.fade * shape * soft;
    return float4((input.color.rgb * tint + haze) * alpha, input.additive != 0 ? 0.0 : alpha);
}
