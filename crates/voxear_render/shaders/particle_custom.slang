// Particle graphs: VoxShade graphs a Particle System picks to move and draw
// its particles, compiled in when a project has any. Each is a number of
// its own (ParticleSystem.graph, 0 for none), which customParticleUpdate
// and customParticleLook call it by.

// A particle as a graph's Update sees it, each step of the simulation: its
// age in seconds and as a share of its life, its number from 0 to 1, how
// fast it goes, where it is and where its system's Obj is, in the world,
// the particles' time and the step's length, in seconds; and the pictures
// its system hands its graph.
struct ParticleUpdateInput
{
    float age;
    float life;
    float seed;
    float3 velocity;
    float3 position;
    float3 objectPosition;
    float time;
    float deltaTime;
    // The pictures its graph reads, by their textures (particleGraphPicture).
    uint3 pictures;
};

// What the Update gives: a force on the particle, in metres a second each
// second, and how fast it goes after it. Left as they come, nothing
// changes.
struct ParticleUpdateOutput
{
    float3 force;
    float3 velocity;
};

// A particle as a graph's Look sees it where it is drawn: as above, with
// the colour and opacity, and width in metres, its system's settings give.
struct ParticleLookInput
{
    float age;
    float life;
    float seed;
    float3 velocity;
    float3 position;
    float3 objectPosition;
    float time;
    float4 color;
    float size;
    uint3 pictures;
};

struct ParticleLookOutput
{
    float3 color;
    float alpha;
    float size;
};

// A particle graph's picture `index` at `uv`, wrapping round, as linear light: white for none. Read at its full
// size, as the step and the vertices have no pixels to judge a smaller one by.
float4 particleGraphPicture(uint index, float2 uv)
{
    if (index == NO_TEXTURE)
    {
        return float4(1.0);
    }
    return textures[NonUniformResourceIndex(index)].SampleLevel(samplers[SAMPLER_LINEAR_REPEAT], uv, 0.0);
}

// A particle's number for its graph, from 0 to 1, from the first of the
// random numbers it is drawn with, the same in the step and where it is drawn.
float particleGraphSeed(uint h)
{
    uint mixed = h ^ 0xA511E9B3u;
    mixed = (mixed ^ (mixed >> 16)) * 0x7FEB352Du;
    mixed = (mixed ^ (mixed >> 15)) * 0x846CA68Bu;
    return float((mixed ^ (mixed >> 16)) >> 8) / 16777216.0;
}

#ifdef VOXEAR_CUSTOM_SHADERS
#include "project_particle_graphs.slang"
#else
void customParticleUpdate(uint graph, ParticleUpdateInput input, inout ParticleUpdateOutput output)
{
}

void customParticleLook(uint graph, ParticleLookInput input, inout ParticleLookOutput output)
{
}
#endif
