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

// Places the quad of a particle at `world` that has lived `aged` of its
// life, `age` seconds, of `system`, at `corner` of its quad, with its
// random numbers `h`; and how bright and opaque it shows, `fade` of it.
ParticleOut particlePlace(Frame* frame, ParticleSystem system, float3 world, float age, float aged, float2 corner, inout uint h, float fade)
{
    ParticleOut output;
    output.corner = corner;
    output.softness = system.softness;
    output.additive = (system.flags & PARTICLES_ADDITIVE) != 0 ? 1 : 0;
    float grown = system.curveCounts.x > 0 ? curveAt(system.sizeCurve, system.curveCounts.x, aged) : lerp(1.0, system.sizeEnd, aged);
    float size = lerp(system.size.x, system.size.y, particleRandom(h)) * grown;
    float turned = particleRandom(h) * 2.0 * PI + system.spin * age * (particleRandom(h) < 0.5 ? -1.0 : 1.0);
    float2 spun = float2(corner.x * cos(turned) - corner.y * sin(turned), corner.x * sin(turned) + corner.y * cos(turned));
    float3 forward = frame->cameraForward;
    float3 right = normalizeOr(cross(float3(0.0, 1.0, 0.0), forward), float3(1.0, 0.0, 0.0));
    float3 up = cross(forward, right);
    float3 at = world + (right * spun.x + up * spun.y) * size * 0.5;
    float4 clip = transform(frame->viewProj, float4(at, 1.0));

    float4 color = lerp(system.color, system.colorEnd, aged);
    if (system.curveCounts.w > 0)
    {
        color.rgb = gradientAt(system.gradient, system.curveCounts.w, aged);
    }
    if (system.curveCounts.y > 0)
    {
        color.a = system.color.a * curveAt(system.opacityCurve, system.curveCounts.y, aged);
    }
    float3 light = (system.flags & PARTICLES_LIT) != 0 ? particleLight(frame, world, transform(frame->viewProj, float4(world, 1.0))) : 1.0;
    // Seen through the air to it: a particle that covers what is behind takes on the fog's light too, while one
    // that adds its light only dims, as the fog's light is behind it already.
    float4 air = particleFog(frame, world, transform(frame->viewProj, float4(world, 1.0)));
    float3 shown = color.rgb * light * system.intensity * air.a + (output.additive != 0 ? 0.0 : air.rgb);
    output.color = float4(shown, saturate(color.a * fade));
    output.depth = clip.w;
    output.position = clip;
    return output;
}


// A particle's pixel: a soft round dot, faded before what it meets, and
// hidden behind it (the scene's depth, push.textures.x).
[shader("fragment")]
float4 particleFragment(ParticleOut input) : SV_Target0
{
    Frame* frame = push.frame;
    float edge = dot(input.corner, input.corner);
    if (edge >= 1.0)
    {
        discard;
    }
    float behind = textures[NonUniformResourceIndex(push.textures.x)].Load(int3(int2(input.position.xy), 0)).r;
    float gap = (behind > 0.0 ? frame->near / behind : 1e4) - input.depth;
    if (gap <= 0.0)
    {
        discard;
    }
    float soft = input.softness > 0.0 ? saturate(gap / input.softness) : 1.0;
    float alpha = input.color.a * (1.0 - edge) * (1.0 - edge) * soft;
    return float4(input.color.rgb * alpha, input.additive != 0 ? 0.0 : alpha);
}
