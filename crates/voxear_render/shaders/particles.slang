// Particles (src/particles.rs): each Particle System drawn as a quad a
// particle, facing the camera, every particle worked out where it is drawn
// from its system's settings, its slot and how long the system has been
// going. A slot's particle sets out again once its last has died, as many
// slots as the system ever has alive. Soft round dots, faded before what
// they meet, lit as smoke is or glowing as sparks do.

#include "shading.slang"

// A system's flags (particles::LOOPING and the rest).
static const uint PARTICLES_LOOPING = 1;
static const uint PARTICLES_ADDITIVE = 2;
static const uint PARTICLES_LIT = 4;

// Its shapes, as particles::EmitShape.
static const uint EMIT_POINT = 0;
static const uint EMIT_SPHERE = 1;
static const uint EMIT_CONE = 2;
static const uint EMIT_BOX = 3;

static const float GRAVITY = 9.81;

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

// A well-mixed number from `v` (PCG).
uint particleHash(uint v)
{
    uint state = v * 747796405u + 2891336453u;
    uint word = ((state >> ((state >> 28u) + 4u)) ^ state) * 277803737u;
    return (word >> 22u) ^ word;
}

// The next of a particle's random numbers, from 0 to 1.
float particleRandom(inout uint h)
{
    h = particleHash(h);
    return float(h) / 4294967296.0;
}

// A direction picked evenly from every way.
float3 anyWay(inout uint h)
{
    float z = particleRandom(h) * 2.0 - 1.0;
    float turn = particleRandom(h) * 2.0 * PI;
    float across = sqrt(max(1.0 - z * z, 0.0));
    return float3(across * cos(turn), z, across * sin(turn));
}

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

// The light falling on a puff of smoke at `world`: the sky's and the GI's,
// and the sun's and the lamps', each as a ball takes it, lit on one side.
float3 particleLight(Frame* frame, float3 world, float4 clip)
{
    bool atmosphere = frame->lighting.sky == SKY_ATMOSPHERE;
    float3 sunLight = atmosphere ? sunThroughAtmosphere(frame) : frame->sunColor;
    float3 toCamera = normalizeOr(frame->cameraPosition - world, float3(0.0, 1.0, 0.0));
    Surface s;
    s.normal = toCamera;
    s.view = toCamera;
    s.roughness = 1.0;
    s.diffuse = 1.0;
    s.f0 = 0.0;
    float3 ambient = ambientLight(frame, world, s, sunLight, atmosphere);
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

[shader("vertex")]
ParticleOut particleVertex(uint vertexId : SV_VertexID)
{
    Frame* frame = push.frame;
    ParticleSystem system = buffers()->particleSystems[push.source];
    uint slot = vertexId / 6;
    static const float2 CORNERS[6] = { float2(-1.0, -1.0), float2(1.0, -1.0), float2(1.0, 1.0), float2(-1.0, -1.0), float2(1.0, 1.0), float2(-1.0, 1.0) };
    float2 corner = CORNERS[vertexId % 6];
    ParticleOut output;
    output.corner = corner;
    output.color = 0.0;
    output.depth = 0.0;
    output.softness = system.softness;
    output.additive = (system.flags & PARTICLES_ADDITIVE) != 0 ? 1 : 0;
    // Out of view, drawing nothing, while its slot has no particle.
    output.position = float4(0.0, 0.0, -1.0, 1.0);

    float spawn;
    uint round;
    if (!particleSpawn(system, slot, spawn, round))
    {
        return output;
    }
    uint h = particleHash(system.seed ^ particleHash(slot ^ particleHash(round * 7919u + 17u)));
    float life = lerp(system.lifetime.x, system.lifetime.y, particleRandom(h));
    float age = system.time - spawn;
    if (age > life)
    {
        return output;
    }
    float aged = age / life;

    // Where it set out from, and which way, in its Obj's own space.
    float3 from = 0.0;
    float3 way = float3(0.0, 1.0, 0.0);
    if (system.shape == EMIT_POINT)
    {
        way = anyWay(h);
    }
    else if (system.shape == EMIT_SPHERE)
    {
        way = anyWay(h);
        from = way * system.radius * pow(particleRandom(h), 1.0 / 3.0);
    }
    else if (system.shape == EMIT_CONE)
    {
        float tilt = acos(lerp(1.0, cos(system.angle), particleRandom(h)));
        float turn = particleRandom(h) * 2.0 * PI;
        way = float3(sin(tilt) * cos(turn), cos(tilt), sin(tilt) * sin(turn));
        float reach = system.radius * sqrt(particleRandom(h));
        float around = particleRandom(h) * 2.0 * PI;
        from = float3(cos(around), 0.0, sin(around)) * reach;
    }
    else
    {
        from = (float3(particleRandom(h), particleRandom(h), particleRandom(h)) - 0.5) * system.boxSize;
    }
    float speed = lerp(system.speed.x, system.speed.y, particleRandom(h));
    float3 start = transform(system.model, float4(from, 1.0)).xyz;
    float3 velocity = transform(system.model, float4(way * speed, 0.0)).xyz;
    // Flying out, slowed by the air and pulled down: x = x0 + v0 (1 - e^-kt) / k + g (t / k - (1 - e^-kt) / k^2).
    float3 pull = float3(0.0, -GRAVITY * system.gravity, 0.0);
    float3 world;
    if (system.drag > 1e-4)
    {
        float k = system.drag;
        float slowed = (1.0 - exp(-k * age)) / k;
        world = start + velocity * slowed + pull * (age - slowed) / k;
    }
    else
    {
        world = start + velocity * age + pull * (0.5 * age * age);
    }

    float size = lerp(system.size.x, system.size.y, particleRandom(h)) * lerp(1.0, system.sizeEnd, aged);
    float turned = particleRandom(h) * 2.0 * PI + system.spin * age * (particleRandom(h) < 0.5 ? -1.0 : 1.0);
    float2 spun = float2(corner.x * cos(turned) - corner.y * sin(turned), corner.x * sin(turned) + corner.y * cos(turned));
    float3 forward = frame->cameraForward;
    float3 right = normalizeOr(cross(float3(0.0, 1.0, 0.0), forward), float3(1.0, 0.0, 0.0));
    float3 up = cross(forward, right);
    float3 at = world + (right * spun.x + up * spun.y) * size * 0.5;
    float4 clip = transform(frame->viewProj, float4(at, 1.0));

    float4 color = lerp(system.color, system.colorEnd, aged);
    float3 light = (system.flags & PARTICLES_LIT) != 0 ? particleLight(frame, world, transform(frame->viewProj, float4(world, 1.0))) : 1.0;
    output.color = float4(color.rgb * light * system.intensity, saturate(color.a));
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
