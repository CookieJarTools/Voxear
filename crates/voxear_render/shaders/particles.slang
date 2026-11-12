// Particles (src/particles.rs): each Particle System drawn as a quad a
// particle, facing the camera, every particle worked out where it is drawn
// from its system's settings, its slot and how long the system has been
// going. A slot's particle sets out again once its last has died, as many
// slots as the system ever has alive. Soft round dots, faded before what
// they meet, lit as smoke is or glowing as sparks do. A simulated system's
// particles are read from the pool instead (src/particle_sim.rs), and drawn
// the same.
//
// A system's particles may leave trails, drawn here too with the same
// pixels: a tail each, a strip along the points the simulation keeps of the
// way each has come (trailVertex); or one ribbon through them all in the
// order they were sent out (ribbonVertex). And a system may draw each
// particle as a mesh, tumbling, lit as any surface is and hiding what is
// behind it (particleMeshVertex, particleMeshFragment).

#include "particle_shared.slang"

// A particle as it is now: where in the world and how fast it goes, how old
// it is in seconds and as a share of its life, and its random numbers.
struct ParticleNow
{
    float3 at;
    float3 going;
    float age;
    float aged;
    uint h;
    // For one drawn as a mesh, how far it has tumbled, in radians, and how fast it is tumbling, in radians a second.
    float turned;
    float turning;
};

// The simulated particle drawn `drawn`th of its system, the oldest first, as the pool has it (particle_sim.slang):
// false when its place holds none.
bool particleKept(ParticleSystem system, uint drawn, out ParticleNow now)
{
    Particle particle = buffers()->particles[system.poolFirst + (system.head + drawn) % system.capacity];
    bool inWorld = (system.flags & PARTICLES_WORLD) != 0;
    now.at = inWorld ? particle.position : transform(system.model, float4(particle.position, 1.0)).xyz;
    now.going = inWorld ? particle.velocity : transform(system.model, float4(particle.velocity, 0.0)).xyz;
    now.age = particle.age;
    now.aged = particle.age / max(particle.life, 1e-6);
    // A mesh particle's number keeps how far it has tumbled in its low half (particle_sim.slang).
    bool meshes = (system.flags & PARTICLES_MESH) != 0;
    now.h = particleHash(meshes ? particle.seed & ~PARTICLE_TURNED : particle.seed);
    now.turned = float(particle.seed & PARTICLE_TURNED) * (2.0 * PI / 65536.0);
    now.turning = abs(system.spin) * particleTumbling(particle.velocity);
    return particle.life > 0.0 && particle.age < particle.life;
}

// The particle in place `slot` of a system: read from the pool for a simulated one, in the order they were sent
// out, so the newest are drawn over the oldest; otherwise worked out from its age. False when the place has none.
bool particleNow(ParticleSystem system, uint slot, out ParticleNow now)
{
    if ((system.flags & PARTICLES_SIMULATED) != 0)
    {
        return particleKept(system, slot, now);
    }
    now.at = 0.0;
    now.going = 0.0;
    now.age = 0.0;
    now.aged = 0.0;
    now.h = 0;
    now.turned = 0.0;
    now.turning = 0.0;
    float spawn;
    uint round;
    if (!particleSpawn(system, slot, spawn, round))
    {
        return false;
    }
    uint h = particleHash(system.seed ^ particleHash(slot ^ particleHash(round * 7919u + 17u)));
    float life = lerp(system.lifetime.x, system.lifetime.y, particleRandom(h));
    float age = system.time - spawn;
    if (age > life)
    {
        return false;
    }
    // Where it set out from, and which way, in its Obj's own space.
    float3 from;
    float3 way;
    particleSetOut(system.shape, system.radius, system.angle, system.boxSize, h, from, way);
    float speed = lerp(system.speed.x, system.speed.y, particleRandom(h));
    float3 start = transform(system.model, float4(from, 1.0)).xyz;
    float3 velocity = transform(system.model, float4(way * speed, 0.0)).xyz;
    now.at = particleFlight(system, start, velocity, age, life);
    now.going = particleSpeedAfter(float3(0.0, -PARTICLE_GRAVITY * system.gravity, 0.0), system.drag, velocity, age);
    now.age = age;
    now.aged = age / life;
    now.h = h;
    // Worked out from its age alone, it tumbles all its life.
    now.turned = abs(system.spin) * age;
    now.turning = abs(system.spin);
    return true;
}

[shader("vertex")]
ParticleOut particleVertex(uint vertexId : SV_VertexID)
{
    Frame* frame = push.frame;
    ParticleSystem system = buffers()->particleSystems[push.source];
    float2 corner = PARTICLE_CORNERS[vertexId % 6];
    ParticleNow now;
    if (!particleNow(system, vertexId / 6, now))
    {
        return particleNone(system, corner);
    }
    return particlePlaceGoing(frame, system, now.at, now.going, now.age, now.aged, corner, now.h, 1.0);
}

// How narrow a trail is drawn at the least, in pixels from its middle to its edge: one thinner than this fades
// instead, as a line thinner than a pixel or two breaks up into dots as it moves.
static const float TRAIL_LEAST = 1.25;

// A corner of a trail: `here` on its way, which runs along `way` there, `wide` across, on the side `side` of it
// (-1 or 1), showing `fade` of the opacity of its particle, which is `point.age` old. Coloured and lit as the
// particle is, there; drawn without its picture, soft at its edges, turned to face the camera along its length.
ParticleOut trailPlace(Frame* frame, ParticleSystem system, float3 here, float3 way, float wide, float side, float fade, ParticleNow point)
{
    ParticleSystem plain = system;
    plain.texture = NO_TEXTURE;
    plain.stretch = 0.0;
    plain.flags &= ~PARTICLES_SIX_WAY;
    uint h = point.h;
    ParticleOut output = particlePlaceGoing(frame, plain, here, 0.0, point.age, point.aged, float2(side, 0.0), h, 1.0);
    float3 across = normalizeOr(cross(way, frame->cameraPosition - here), 0.0);
    float4 middle = transform(frame->viewProj, float4(here, 1.0));
    float4 clip = transform(frame->viewProj, float4(here + across * (side * wide * 0.5), 1.0));
    // Where a metre out on that side is on screen: which way its edge lies, and how many pixels a metre is there.
    float4 metreOut = transform(frame->viewProj, float4(here + across * side, 1.0));
    if (middle.w > frame->near && metreOut.w > frame->near && dot(across, across) > 0.0)
    {
        float2 metre = (metreOut.xy / metreOut.w - middle.xy / middle.w) * 0.5 * float2(frame->viewport);
        float pixels = length(metre);
        float half = pixels * wide * 0.5;
        if (half < TRAIL_LEAST && pixels > 1e-4)
        {
            // Its edge moved out to the least, and as much less of it shown.
            float2 edge = middle.xy / middle.w + metre / pixels * (TRAIL_LEAST * 2.0) / float2(frame->viewport);
            clip = float4(edge * middle.w, middle.z, middle.w);
            fade *= half / TRAIL_LEAST;
        }
    }
    output.position = clip;
    output.depth = clip.w;
    output.fade = output.color.a * fade;
    output.color.a = 1.0;
    return output;
}

// Point `n` of the tail of the particle in place `slot` of its system's room, from its head back: where the
// particle is, at `head`, then where each tick found it, the newest first, and last the oldest kept, drawn in
// toward the one after it as the tick goes by, so the tail's end slides rather than jumps. In the world.
float3 tailPoint(ParticleSystem system, uint slot, float3 head, uint n)
{
    if (n == 0)
    {
        return head;
    }
    uint first = (system.poolFirst + system.capacity) * 3 + slot * PARTICLE_TRAIL_POINTS;
    uint newest = system.trailTick + PARTICLE_TRAIL_POINTS * 2;
    uint back = min(n, PARTICLE_TRAIL_POINTS) - 1;
    float3 point = buffers()->particleTrails[first + (newest - back) % PARTICLE_TRAIL_POINTS];
    if (n >= PARTICLE_TRAIL_POINTS)
    {
        point = lerp(point, buffers()->particleTrails[first + (newest - back + 1) % PARTICLE_TRAIL_POINTS], system.trailPart);
    }
    return (system.flags & PARTICLES_WORLD) != 0 ? point : transform(system.model, float4(point, 1.0)).xyz;
}

// The tails of a system whose particles draw them (PARTICLES_TRAILS): for each particle a strip of
// PARTICLE_TRAIL_POINTS pieces from where it is back along the way it has come, narrowing and fading to nothing.
[shader("vertex")]
ParticleOut trailVertex(uint vertexId : SV_VertexID)
{
    Frame* frame = push.frame;
    ParticleSystem system = buffers()->particleSystems[push.source];
    uint each = PARTICLE_TRAIL_POINTS * 6;
    uint drawn = vertexId / each;
    uint within = vertexId % each;
    float2 corner = PARTICLE_CORNERS[within % 6];
    ParticleNow now;
    if (!particleKept(system, drawn, now))
    {
        return particleNone(system, corner);
    }
    uint slot = (system.head + drawn) % system.capacity;
    uint n = within / 6 + (corner.y > 0.0 ? 1 : 0);
    float3 here = tailPoint(system, slot, now.at, n);
    float3 ahead = tailPoint(system, slot, now.at, n == 0 ? 0 : n - 1);
    float3 behind = tailPoint(system, slot, now.at, min(n + 1, PARTICLE_TRAIL_POINTS));
    // How far along the tail, from its head to its end.
    float along = n == 0 ? 0.0 : n >= PARTICLE_TRAIL_POINTS ? 1.0 : (system.trailPart + float(n - 1)) / float(PARTICLE_TRAIL_POINTS - 1);
    float wide = particleWide(system, now.aged, now.h) * system.trailWidth * (1.0 - along);
    return trailPlace(frame, system, here, ahead - behind, wide, corner.x, 1.0 - along, now);
}

// Point `n` of a system's ribbon: the particle drawn `n`th, the oldest first; and after the newest, the system's
// Obj itself while it is still sending particles out, so the ribbon reaches all the way to it. False where there
// is none.
bool ribbonPoint(ParticleSystem system, uint n, out ParticleNow point)
{
    if (n < system.capacity)
    {
        return particleKept(system, n, point);
    }
    point.at = system.model.c3.xyz;
    point.going = 0.0;
    point.age = 0.0;
    point.aged = 0.0;
    point.turned = 0.0;
    point.turning = 0.0;
    point.h = particleHash(system.seed);
    return n == system.capacity && (system.flags & PARTICLES_RIBBON_FED) != 0;
}

// The ribbon of a system that joins its particles in one (PARTICLES_RIBBON): a piece between each particle and
// the next sent out, as wide and coloured at each as that particle is, so it fades and narrows as they age.
[shader("vertex")]
ParticleOut ribbonVertex(uint vertexId : SV_VertexID)
{
    Frame* frame = push.frame;
    ParticleSystem system = buffers()->particleSystems[push.source];
    uint piece = vertexId / 6;
    float2 corner = PARTICLE_CORNERS[vertexId % 6];
    ParticleNow from;
    ParticleNow to;
    bool whole = ribbonPoint(system, piece, from);
    whole = ribbonPoint(system, piece + 1, to) && whole;
    if (!whole)
    {
        return particleNone(system, corner);
    }
    bool later = corner.y > 0.0;
    ParticleNow here;
    if (later)
    {
        here = to;
    }
    else
    {
        here = from;
    }
    // Which way the ribbon runs here: from the point before to the point after, where there are any.
    float3 before = from.at;
    float3 after = to.at;
    ParticleNow beyond;
    if (later && ribbonPoint(system, piece + 2, beyond))
    {
        after = beyond.at;
    }
    if (!later && piece > 0 && ribbonPoint(system, piece - 1, beyond))
    {
        before = beyond.at;
    }
    float wide = particleWide(system, here.aged, here.h) * system.trailWidth;
    return trailPlace(frame, system, here.at, after - before, wide, corner.x, 1.0, here);
}

// A corner of a particle drawn as a mesh.
struct MeshParticleOut
{
    float4 position : SV_Position;
    float3 world : WORLD;
    float3 normal : NORMAL;
    float2 uv : UV;
    // Its colour, as bright as it glows, and how opaque it is.
    nointerpolation float4 color : COLOR;
    // The fog's light over it, and in alpha how much of it shows through the air.
    nointerpolation float4 air : AIR;
    nointerpolation uint material : MATERIAL;
    nointerpolation uint lit : LIT;
    // Where it is on screen without the frame's jitter, and where it was a frame ago, for its motion.
    float4 currentClip : CURRENT;
    float4 previousClip : PREVIOUS;
};

// `v` turned `angle` round `axis`.
float3 turnedAbout(float3 v, float3 axis, float angle)
{
    return v * cos(angle) + cross(axis, v) * sin(angle) + axis * (dot(axis, v) * (1.0 - cos(angle)));
}

// A system whose particles are meshes (PARTICLES_MESH): each instance a particle, its mesh as wide as the
// particle's size across its furthest corners, turned some way of its own and tumbling round an axis of its own
// as fast as the system's Spin says; a simulated one as fast as it is going, so it lies still once at rest.
[shader("vertex")]
MeshParticleOut particleMeshVertex(uint vertexId : SV_VertexID, uint slot : SV_InstanceID)
{
    Frame* frame = push.frame;
    ParticleSystem system = buffers()->particleSystems[push.source];
    MeshParticleOut output;
    output.position = float4(0.0, 0.0, -1.0, 1.0);
    output.world = 0.0;
    output.normal = float3(0.0, 1.0, 0.0);
    output.uv = 0.0;
    output.color = 0.0;
    output.air = float4(0.0, 0.0, 0.0, 1.0);
    output.material = 0;
    output.lit = 0;
    output.currentClip = float4(0.0, 0.0, 0.0, 1.0);
    output.previousClip = float4(0.0, 0.0, 0.0, 1.0);
    ParticleNow now;
    if (!particleNow(system, slot, now))
    {
        return output;
    }
    Mesh mesh = buffers()->meshes[system.mesh];
    Vertex v = buffers()->vertices[mesh.vertexOffset + vertexId];
    float wide = particleWide(system, now.aged, now.h);
    uint own = particleHash(now.h ^ 0x9E3779B9u);
    float3 axis = anyWay(own);
    float started = particleRandom(own) * 2.0 * PI;
    float way = particleRandom(own) < 0.5 ? -1.0 : 1.0;
    float turning = now.turning * way;
    float angle = started + now.turned * way;
    float3 local = (v.position - mesh.center) * (wide / max(mesh.radius * 2.0, 1e-5));
    float3 world = now.at + turnedAbout(local, axis, angle);
    float4 clip = transform(frame->viewProj, float4(world, 1.0));
    // Where this corner was a frame ago: its particle back along its way, turned as it was then.
    float ago = min(frame->deltaTime, now.age);
    float3 before = now.at - now.going * ago + turnedAbout(local, axis, angle - turning * ago);
    output.currentClip = transform(frame->unjitteredViewProj, float4(world, 1.0));
    output.previousClip = transform(frame->prevViewProj, float4(before, 1.0));

    float4 color = lerp(system.color, system.colorEnd, now.aged);
    if (system.curveCounts.w > 0)
    {
        color.rgb = gradientAt(system.gradient, system.curveCounts.w, now.aged);
    }
    if (system.curveCounts.y > 0)
    {
        color.a = system.color.a * curveAt(system.opacityCurve, system.curveCounts.y, now.aged);
    }
    output.position = clip;
    output.world = world;
    output.normal = turnedAbout(v.normal, axis, angle);
    output.uv = v.uv;
    output.color = float4(color.rgb * system.intensity, saturate(color.a));
    output.air = particleFog(frame, now.at, transform(frame->viewProj, float4(now.at, 1.0)));
    output.material = system.meshMaterial;
    output.lit = (system.flags & PARTICLES_LIT) != 0 ? 1 : 0;
    return output;
}

// A mesh particle's pixel: its mesh's own material, its colour, how rough and how much metal, under the system's
// colour; lit by the sun, the sky and the lamps as any surface is, or glowing as it is when the system is not lit.
// As it fades it thins out, pixel by pixel, rather than turning clear, so it still hides what is behind it.
[shader("fragment")]
FragmentOut particleMeshFragment(MeshParticleOut input)
{
    Frame* frame = push.frame;
    if (input.color.a < 1.0 && bayer4(uint2(input.position.xy)) >= input.color.a)
    {
        discard;
    }
    FragmentOut output;
    output.velocity = motion(input.currentClip, input.previousClip);
    Material material = buffers()->materials[input.material];
    float3 albedo = input.color.rgb * material.baseColor.rgb * sampleTexture(material.baseColorTexture, input.uv).rgb;
    if (input.lit == 0)
    {
        output.color = float4(albedo * input.air.a + input.air.rgb, 1.0);
        return output;
    }
    Surface s;
    s.normal = normalizeOr(input.normal, float3(0.0, 1.0, 0.0));
    s.view = normalizeOr(frame->cameraPosition - input.world, float3(0.0, 1.0, 0.0));
    float4 metalRough = sampleTexture(material.metallicRoughnessTexture, input.uv);
    float metallic = saturate(material.metallic * metalRough.b);
    s.diffuse = albedo * (1.0 - metallic);
    s.f0 = lerp(float3(0.04), albedo, metallic);
    s.roughness = clamp(material.roughness * metalRough.g, 0.05, 1.0);
    bool atmosphere = frame->lighting.sky == SKY_ATMOSPHERE;
    float3 sunLight = atmosphere ? sunThroughAtmosphere(frame) : frame->sunColor;
    float3 lit = ambientLight(frame, input.world, s, sunLight, atmosphere) * s.diffuse;
    lit += shade(s, -frame->sunDirection, sunLight * sunCookieAt(frame, input.world)) * sunShadow(frame, input.world, s.normal);
    lit += localLights(frame, s, input.world, input.position.xy);
    output.color = float4(lit * input.air.a + input.air.rgb, 1.0);
    return output;
}
