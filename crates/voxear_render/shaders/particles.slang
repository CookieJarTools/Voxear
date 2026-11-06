// Particles (src/particles.rs): each Particle System drawn as a quad a
// particle, facing the camera, every particle worked out where it is drawn
// from its system's settings, its slot and how long the system has been
// going. A slot's particle sets out again once its last has died, as many
// slots as the system ever has alive. Soft round dots, faded before what
// they meet, lit as smoke is or glowing as sparks do.

#include "particle_shared.slang"

[shader("vertex")]
ParticleOut particleVertex(uint vertexId : SV_VertexID)
{
    Frame* frame = push.frame;
    ParticleSystem system = buffers()->particleSystems[push.source];
    uint slot = vertexId / 6;
    float2 corner = PARTICLE_CORNERS[vertexId % 6];
    ParticleOut output = particleNone(system, corner);
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
    float3 world = particleFlight(system, start, velocity, age);
    return particlePlace(frame, system, world, age, aged, corner, h, 1.0);
}

