// Sound rays (src/sound_rays.rs): rays traced for the game's sound against
// the scene the GI traces, by the GPU's ray queries or the compute fallback,
// as ddgi.slang's traceScene does, which includes this after it.
//
// - soundOcclusionMain: for each source, rays from the listener to points
//   across its sphere. A ray that meets nothing is open; one that meets
//   walls brings through what their acoustic materials let through, a wall
//   at a time.
// - soundReverbMain: rays from a spot, the listener's or a reverb probe's,
//   bounced round the scene as Steam Audio's reflection simulation bounces
//   its own: at each surface, what of the sound reaches the spot again from
//   there is added to the spot's energy field, by when it arrives, its band
//   of pitch, and which way the ray left. The field is added up in shared
//   memory a workgroup at a time, in whole numbers of SOUND_FIXED a unit.

#pragma once

static const uint SOUND_BINS = 200;
static const uint SOUND_BANDS = 3;
static const uint SOUND_CHANNELS = 4;
static const uint SOUND_FIELD = SOUND_CHANNELS * SOUND_BANDS * SOUND_BINS;
static const uint SOUND_MAX_SPOTS = 9;
static const uint SOUND_MAX_SOURCES = 64;
static const uint SOUND_MATERIALS = 16;
static const float SOUND_FIXED = 16777216.0;
static const float SOUND_SPEED = 340.0;
static const float SOUND_BIN_SECONDS = 0.01;
// Steam Audio's: the listener's radius, how far off a surface a bounce
// starts, the specular lobe's sharpness, and the nearest a surface counts
// as, for its energy.
static const float SOUND_LISTENER_RADIUS = 0.1;
static const float SOUND_SURFACE_OFFSET = 0.01;
static const float SOUND_SPECULAR = 100.0;
static const float SOUND_NEAREST = 1.0;
// The most surfaces an occlusion ray passes through, and the most walls it
// lets anything through.
static const uint SOUND_SURFACES = 6;
// The most bounces a ray goes on with past its own, in its tail.
static const uint SOUND_TAIL = 256;

// The acoustic presets, in voxear_physics::scene::Acoustic's order.
static const uint ACOUSTIC_GENERIC = 0;
static const uint ACOUSTIC_GLASS = 6;
static const uint ACOUSTIC_METAL = 9;

struct SoundMaterial
{
    // xyz how much of each band a surface soaks up, w how much it scatters.
    float4 absorption;
    // xyz how much of each band passes through it.
    float4 transmission;
};

struct SoundSource
{
    float4 sphere;
    uint rays;
    uint3 pad;
};

struct SoundJob
{
    float4 listener;
    float4 spots[SOUND_MAX_SPOTS];
    SoundSource sources[SOUND_MAX_SOURCES];
    SoundMaterial materials[SOUND_MATERIALS];
    uint spotCount;
    uint sourceCount;
    uint rays;
    uint bounces;
    uint seed;
    float duration;
    uint2 pad;
};

struct SoundHeard
{
    int fields[SOUND_MAX_SPOTS * SOUND_FIELD];
    // Per source: x its open rays, yzw what its blocked ones brought through, in each band.
    int4 sources[SOUND_MAX_SOURCES];
};

SoundJob* soundJob()
{
    return (SoundJob*)(uint64_t(push.textures.x) | (uint64_t(push.textures.y) << 32));
}

SoundHeard* soundHeard()
{
    return (SoundHeard*)(uint64_t(push.textures.z) | (uint64_t(push.textures.w) << 32));
}

uint soundHash(uint v)
{
    v = v * 747796405u + 2891336453u;
    uint word = ((v >> ((v >> 28u) + 4u)) ^ v) * 277803737u;
    return (word >> 22u) ^ word;
}

float soundRandom(inout uint state)
{
    state = soundHash(state);
    return float(state >> 8) / 16777216.0;
}

// The acoustic material a hit instance is made of: its own, given for a
// chunk of blocks; else its material's; else a guess from how it looks.
uint soundAcoustic(uint id)
{
    Instance instance = buffers()->instances[id];
    if (instance.acoustic != 0)
    {
        return min(instance.acoustic - 1, SOUND_MATERIALS - 1);
    }
    Material material = buffers()->materials[instance.material];
    if (material.acoustic != 0)
    {
        return min(material.acoustic - 1, SOUND_MATERIALS - 1);
    }
    if ((material.flags & MATERIAL_GLASS) != 0)
    {
        return ACOUSTIC_GLASS;
    }
    return material.metallic > 0.5 ? ACOUSTIC_METAL : ACOUSTIC_GENERIC;
}

// Whether sound passes a hit instance as though it were not there: a card
// of leaves, a cut-out fence, foliage.
bool soundPassesThrough(uint id)
{
    Material material = buffers()->materials[buffers()->instances[id].material];
    return material.alphaCutoff > 0.0 || (material.flags & MATERIAL_FOLIAGE) != 0;
}

// A hit's surface's way out, in the world, turned to face the ray.
float3 soundNormal(Hit hit, float3 direction)
{
    Instance instance = buffers()->instances[hit.instance];
    Mesh mesh = buffers()->meshes[instance.mesh];
    uint first = mesh.lods[0].indexOffset + hit.primitive * 3;
    float3 a = buffers()->vertices[mesh.vertexOffset + buffers()->indices[first]].position;
    float3 b = buffers()->vertices[mesh.vertexOffset + buffers()->indices[first + 1]].position;
    float3 c = buffers()->vertices[mesh.vertexOffset + buffers()->indices[first + 2]].position;
    float3 local = cross(b - a, c - a);
    float3 normal = instance.normal0.xyz * local.x + instance.normal1.xyz * local.y + instance.normal2.xyz * local.z;
    normal = normalize(select(dot(normal, normal) > 1e-20, normal, -direction));
    return dot(normal, direction) > 0.0 ? -normal : normal;
}

// Where a ray from `origin` along `direction` enters a sphere, or -1.
float soundEnterSphere(float3 origin, float3 direction, float3 centre, float radius)
{
    float3 to = origin - centre;
    float b = dot(to, direction);
    float c = dot(to, to) - radius * radius;
    float h = b * b - c;
    if (h < 0.0)
    {
        return -1.0;
    }
    float near = -b - sqrt(h);
    return near >= 0.0 ? near : (-b + sqrt(h) >= 0.0 ? 0.0 : -1.0);
}

// A thread a ray, a workgroup a source: each ray from the listener to a
// point on the source's sphere, across a disc facing the listener.
[shader("compute")]
[numthreads(16, 1, 1)]
void soundOcclusionMain(uint3 group : SV_GroupID, uint3 thread : SV_GroupThreadID)
{
    Frame* frame = push.frame;
    SoundJob* job = soundJob();
    if (group.y >= job->sourceCount)
    {
        return;
    }
    SoundSource source = job->sources[group.y];
    if (thread.x >= source.rays)
    {
        return;
    }
    float3 listener = job->listener.xyz;
    float3 toward = source.sphere.xyz - listener;
    float far = length(toward);
    float3 target = source.sphere.xyz;
    if (source.rays > 1 && far > 1e-3)
    {
        // A golden-angle spiral over the disc.
        float3 ahead = toward / far;
        float3 side = normalize(cross(abs(ahead.y) < 0.99 ? float3(0.0, 1.0, 0.0) : float3(1.0, 0.0, 0.0), ahead));
        float3 up = cross(ahead, side);
        float r = source.sphere.w * sqrt((float(thread.x) + 0.5) / float(source.rays));
        float turn = float(thread.x) * 2.39996323;
        target += (side * cos(turn) + up * sin(turn)) * r;
    }
    float3 way = target - listener;
    float left = length(way);
    if (left < 1e-4)
    {
        InterlockedAdd(soundHeard()->sources[group.y].x, 1);
        return;
    }
    way /= left;
    float3 origin = listener;
    float3 through = 1.0;
    uint surfaces = 0;
    for (uint i = 0; i < SOUND_SURFACES; i++)
    {
        Hit hit = traceScene(frame, origin, way, left, false);
        if (!hit.found)
        {
            break;
        }
        float step = hit.t + SOUND_SURFACE_OFFSET;
        origin += way * step;
        left -= step;
        if (soundPassesThrough(hit.instance))
        {
            continue;
        }
        // A wall's two faces are one wall: what enters it is what comes out.
        if (surfaces % 2 == 0)
        {
            through *= job->materials[soundAcoustic(hit.instance)].transmission.xyz;
        }
        surfaces++;
        if (left <= 0.0)
        {
            break;
        }
    }
    SoundHeard* heard = soundHeard();
    if (surfaces == 0)
    {
        InterlockedAdd(heard->sources[group.y].x, 1);
        return;
    }
    // More walls than were followed let nothing through.
    through = surfaces >= SOUND_SURFACES ? 0.0 : through;
    int3 fixed = int3(saturate(through) * SOUND_FIXED);
    InterlockedAdd(heard->sources[group.y].y, fixed.x);
    InterlockedAdd(heard->sources[group.y].z, fixed.y);
    InterlockedAdd(heard->sources[group.y].w, fixed.z);
}

groupshared int soundField[SOUND_FIELD];

// A spot's rays a workgroup at a time, 64 rays a workgroup; each spot a row
// of workgroups.
[shader("compute")]
[numthreads(64, 1, 1)]
void soundReverbMain(uint3 group : SV_GroupID, uint3 thread : SV_GroupThreadID)
{
    Frame* frame = push.frame;
    SoundJob* job = soundJob();
    for (uint k = thread.x; k < SOUND_FIELD; k += 64)
    {
        soundField[k] = 0;
    }
    GroupMemoryBarrierWithGroupSync();
    uint ray = group.x * 64 + thread.x;
    if (group.y < job->spotCount && ray < job->rays)
    {
        float3 spot = job->spots[group.y].xyz;
        uint state = soundHash(ray * 9781u + job->seed * 6271u + group.y * 31u);
        // A Fibonacci sphere of the frame's rays, turned each frame.
        float z = 1.0 - (float(ray) + 0.5) * 2.0 / float(job->rays);
        float around = float(ray) * 2.39996323 + float(job->seed & 1023u) * 0.618034 * 6.2831853;
        float ring = sqrt(max(0.0, 1.0 - z * z));
        float3 leaving = float3(ring * cos(around), z, ring * sin(around));
        float3 direction = leaving;
        float3 origin = spot;
        float3 energy = 1.0;
        float travelled = 0.0;
        float scale = 4.0 * PI / float(job->rays);
        // What each bounce sent back, against the energy the ray had, added up, and the bounces it made.
        float returned = 0.0;
        uint bounced = 0;
        for (uint bounce = 0; bounce < job->bounces; bounce++)
        {
            Hit hit = traceScene(frame, origin, direction, 1e4, false);
            if (!hit.found || travelled > job->duration * SOUND_SPEED || hit.t <= SOUND_LISTENER_RADIUS)
            {
                break;
            }
            // A ray back through the spot has come home.
            if (bounce > 0)
            {
                float home = soundEnterSphere(origin, direction, spot, SOUND_LISTENER_RADIUS);
                if (home >= 0.0 && home < hit.t)
                {
                    break;
                }
            }
            float3 normal = soundNormal(hit, direction);
            float3 at = origin + direction * hit.t + normal * SOUND_SURFACE_OFFSET;
            SoundMaterial material = job->materials[soundAcoustic(hit.instance)];
            float scattering = material.absorption.w;
            float3 kept = 1.0 - material.absorption.xyz;
            float3 back = spot - at;
            float distance = length(back);
            if (dot(normal, back) > 0.0 && distance > SOUND_NEAREST)
            {
                float3 toward = back / distance;
                if (!traceScene(frame, at, toward, distance, true).found)
                {
                    float diffuse = (1.0 / PI) * scattering * max(dot(normal, toward), 0.0);
                    float3 halfway = normalize((toward - direction) * 0.5);
                    float specular = ((SOUND_SPECULAR + 2.0) / (8.0 * PI)) * (1.0 - scattering)
                        * pow(max(dot(halfway, normal), 0.0), SOUND_SPECULAR);
                    float near = 1.0 / max(distance, SOUND_NEAREST);
                    float share = scale * (near * near / (4.0 * PI)) * (diffuse + specular);
                    returned += share;
                    float3 heard = share * kept * energy;
                    uint bin = uint(floor((travelled + hit.t + distance) / SOUND_SPEED / SOUND_BIN_SECONDS));
                    if (bin < SOUND_BINS)
                    {
                        float4 weights = float4(1.0, leaving);
                        for (uint channel = 0; channel < SOUND_CHANNELS; channel++)
                        {
                            for (uint band = 0; band < SOUND_BANDS; band++)
                            {
                                int value = int(heard[band] * weights[channel] * SOUND_FIXED);
                                if (value != 0)
                                {
                                    InterlockedAdd(soundField[(channel * SOUND_BANDS + band) * SOUND_BINS + bin], value);
                                }
                            }
                        }
                    }
                }
            }
            energy *= kept;
            travelled += hit.t;
            origin = at;
            bounced++;
            if (soundRandom(state) < scattering)
            {
                // Cosine-weighted, round the surface's way out.
                float u = soundRandom(state);
                float v = soundRandom(state) * 2.0 * PI;
                float3 side = normalize(cross(abs(normal.y) < 0.99 ? float3(0.0, 1.0, 0.0) : float3(1.0, 0.0, 0.0), normal));
                float3 up = cross(normal, side);
                direction = normalize(side * (sqrt(u) * cos(v)) + up * (sqrt(u) * sin(v)) + normal * sqrt(1.0 - u));
            }
            else
            {
                direction = reflect(direction, normal);
            }
        }
        // A ray still in the room when its bounces run out goes on as it went, so a hard room's ring is not cut
        // short: a bounce every mean free path it has made so far, each keeping what its bounces kept on average,
        // each sending back what they sent back on average, heard from all round.
        if (bounced == job->bounces && bounced > 0 && travelled > 0.0)
        {
            float step = travelled / float(bounced);
            float share = returned / float(bounced);
            float3 keep = pow(max(energy, 1e-12), 1.0 / float(bounced));
            float3 left = energy;
            float far = job->duration * SOUND_SPEED;
            for (uint tail = 1; tail <= SOUND_TAIL && travelled + float(tail) * step < far; tail++)
            {
                left *= keep;
                uint bin = uint(floor((travelled + float(tail) * step) / SOUND_SPEED / SOUND_BIN_SECONDS));
                if (bin >= SOUND_BINS || max(left.x, max(left.y, left.z)) < 1e-6)
                {
                    break;
                }
                for (uint band = 0; band < SOUND_BANDS; band++)
                {
                    int value = int(share * left[band] * SOUND_FIXED);
                    if (value != 0)
                    {
                        InterlockedAdd(soundField[band * SOUND_BINS + bin], value);
                    }
                }
            }
        }
    }
    GroupMemoryBarrierWithGroupSync();
    if (group.y >= job->spotCount)
    {
        return;
    }
    SoundHeard* heard = soundHeard();
    uint base = group.y * SOUND_FIELD;
    for (uint k = thread.x; k < SOUND_FIELD; k += 64)
    {
        int value = soundField[k];
        if (value != 0)
        {
            InterlockedAdd(heard->fields[base + k], value);
        }
    }
}
