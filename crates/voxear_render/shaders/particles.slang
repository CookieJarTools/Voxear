// Particles (src/particles.rs): each Particle System drawn as a quad a
// particle, facing the camera, every particle worked out where it is drawn
// from its system's settings, its slot and how long the system has been
// going. A slot's particle sets out again once its last has died, as many
// slots as the system ever has alive. Soft round dots, faded before what
// they meet, lit as smoke is or glowing as sparks do. A simulated system's
// particles are read from the pool instead (src/particle_sim.rs), and drawn
// the same.

#include "particle_shared.slang"

[shader("vertex")]
ParticleOut particleVertex(uint vertexId : SV_VertexID)
{
    Frame* frame = push.frame;
    ParticleSystem system = buffers()->particleSystems[push.source];
    uint slot = vertexId / 6;
    float2 corner = PARTICLE_CORNERS[vertexId % 6];
    ParticleOut output = particleNone(system, corner);
    if ((system.flags & PARTICLES_SIMULATED) != 0)
    {
        // Kept in the pool and stepped each frame (particle_sim.slang): drawn oldest first, so the newest are over
        // them, in the order they were sent out.
        Particle particle = buffers()->particles[system.poolFirst + (system.head + slot) % system.capacity];
        if (particle.life <= 0.0 || particle.age >= particle.life)
        {
            return output;
        }
        float3 at = (system.flags & PARTICLES_WORLD) != 0 ? particle.position : transform(system.model, float4(particle.position, 1.0)).xyz;
        uint own = particleHash(particle.seed);
        return particlePlace(frame, system, at, particle.age, particle.age / particle.life, corner, own, 1.0);
    }
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
    float3 from;
    float3 way;
    particleSetOut(system.shape, system.radius, system.angle, system.boxSize, h, from, way);
    float speed = lerp(system.speed.x, system.speed.y, particleRandom(h));
    float3 start = transform(system.model, float4(from, 1.0)).xyz;
    float3 velocity = transform(system.model, float4(way * speed, 0.0)).xyz;
    float3 world = particleFlight(system, start, velocity, age, life);
    return particlePlace(frame, system, world, age, aged, corner, h, 1.0);
}

