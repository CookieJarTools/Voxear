// Grass (src/grass.rs): each view scatters each terrain's grass round its
// camera on the GPU (grassScatter), one blade at most in each cell of a
// grid fixed to the world, so a blade stands in the same place, turned and
// sized the same, whenever the camera comes back. A blade grows in a cell
// as likely as its kind grows thickly there, on the ground's height, and
// leaning with it; not in holes, not out of view, and thinning out over
// the last fifth of its draw distance. Blades go into the view's grass
// instances, a run for each kind and level of detail, counted in the
// indirect draws that draw them (grassVertex), with the forward passes'
// own fragment shaders: no instance of the scene's each, so there can be
// millions. They sway in the wind and bend from what passes through them,
// as Foliage does, and lie flat where a Track Maker trod, standing up again
// over their kind's time (grassTrodden); they cast no shadow.

#include "shading.slang"
#include "culling.slang"
#include "terrain_ground.slang"

// A cell's own numbers, 0 to 1, the same every frame.
float grassHash(int2 cell, uint seed, uint salt)
{
    uint h = uint(cell.x) * 0x8DA6B343u ^ uint(cell.y) * 0xD8163841u ^ seed * 0xCB1AB31Fu ^ salt * 0x9E3779B9u;
    h ^= h >> 16;
    h *= 0x7FEB352Du;
    h ^= h >> 15;
    h *= 0x846CA68Bu;
    h ^= h >> 16;
    return float(h >> 8) / 16777216.0;
}

[shader("compute")]
[numthreads(8, 8, 1)]
void grassScatter(uint3 id : SV_DispatchThreadID)
{
    GrassJob job = buffers()->grassJobs[push.list];
    if (any(id.xy >= job.cells))
    {
        return;
    }
    Frame* frame = push.frame;
    int2 cell = job.firstCell + int2(id.xy);
    uint seed = job.terrain * 64 + job.kind;
    float2 at = (float2(cell) + float2(grassHash(cell, seed, 0), grassHash(cell, seed, 1))) * job.cellSize;
    float away = distance(at, frame->cameraPosition.xz);
    if (away > job.drawDistance || grassHash(cell, seed, 2) < smoothstep(job.drawDistance * 0.8, job.drawDistance, away))
    {
        return;
    }
    Terrain terrain = buffers()->terrains[job.terrain];
    float2 local = at - terrain.corner.xz;
    float2 size = float2(terrain.tiles * TERRAIN_TILE_QUADS) * terrain.spacing;
    if (any(local < 0.0) || any(local > size) || grassHash(cell, seed, 3) >= terrainGrass(terrain, local, job.kind))
    {
        return;
    }
    if (terrainCut(terrain, local) && !terrainDugColumn(terrain, local))
    {
        return;
    }
    float3 foot = float3(at.x, terrain.corner.y + terrainHeight(terrain, local), at.y);
    Mesh mesh = buffers()->meshes[job.mesh];
    float grown = grassHash(cell, seed, 4);
    float scale = lerp(job.scale.x, job.scale.y, grown);
    if (!insidePlanes(frame->planes, foot + mesh.center * scale, mesh.radius * scale))
    {
        return;
    }
    uint lod = 0;
    while (lod + 1 < job.lodCount && away > job.lodReach[lod])
    {
        lod++;
    }
    // Leaning half way to square with the ground, which is enough to stand grass on a slope.
    float step = terrain.spacing;
    float dx = terrainHeight(terrain, local + float2(step, 0.0)) - terrainHeight(terrain, local - float2(step, 0.0));
    float dz = terrainHeight(terrain, local + float2(0.0, step)) - terrainHeight(terrain, local - float2(0.0, step));
    float3 up = normalize(lerp(float3(0.0, 1.0, 0.0), normalize(float3(-dx, 2.0 * step, -dz)), 0.5));
    uint turn = uint(grassHash(cell, seed, 5) * 1023.0);
    uint sized = uint(grown * 255.0);
    uint leanX = uint(clamp(up.x * 63.0 + 64.0, 0.0, 127.0));
    uint leanZ = uint(clamp(up.z * 63.0 + 64.0, 0.0, 127.0));
    uint slot;
    InterlockedAdd(buffers()->grassArgs[job.draws + lod].instanceCount, 1, slot);
    if (slot >= job.capacities[lod])
    {
        return;
    }
    GrassBlade blade;
    blade.position = foot;
    blade.packed = turn | sized << 10 | leanX << 18 | leanZ << 25;
    buffers()->grassBlades[job.offsets[lod] + slot] = blade;
}

// Each draw's command made afresh from its mesh's LOD, counting no blades yet; push.list is how many draws.
[shader("compute")]
[numthreads(64, 1, 1)]
void grassReset(uint3 id : SV_DispatchThreadID)
{
    if (id.x >= push.list)
    {
        return;
    }
    GrassJob job = buffers()->grassJobs[buffers()->grassDraws[id.x].job];
    Mesh mesh = buffers()->meshes[job.mesh];
    // The coarsest LOD whose error, at the ring's near edge and the kind's middle size, stays within the frame's
    // threshold, twice as coarse as other meshes': a blade far off is a speck either way.
    uint ring = id.x - job.draws;
    float near = ring == 0 ? 0.0 : job.lodReach[ring - 1];
    float size = (job.scale.x + job.scale.y) * 0.5;
    Frame* frame = push.frame;
    uint pick = 0;
    for (uint each = 1; each < mesh.lodCount; each++)
    {
        if (mesh.lods[each].error * size * frame->lodScale > frame->lodThreshold * 2.0 * max(near, 1e-3))
        {
            break;
        }
        pick = each;
    }
    MeshLod lod = mesh.lods[pick];
    DrawCommand command;
    command.indexCount = lod.indexCount;
    command.instanceCount = 0;
    command.firstIndex = lod.indexOffset;
    command.vertexOffset = 0;
    command.firstInstance = 0;
    buffers()->grassArgs[id.x] = command;
}

// Each draw's count kept within its room, which a scatter that found more could pass.
[shader("compute")]
[numthreads(64, 1, 1)]
void grassClamp(uint3 id : SV_DispatchThreadID)
{
    if (id.x >= push.list)
    {
        return;
    }
    GrassJob job = buffers()->grassJobs[buffers()->grassDraws[id.x].job];
    uint room = job.capacities[id.x - job.draws];
    buffers()->grassArgs[id.x].instanceCount = min(buffers()->grassArgs[id.x].instanceCount, room);
}

// A blade's turn: leaning as it was packed, then turned about its own up.
float3x3 grassTurn(uint packed)
{
    float yaw = float(packed & 0x3FF) / 1023.0 * 6.28318530718;
    float2 lean = (float2(float((packed >> 18) & 0x7F), float((packed >> 25) & 0x7F)) - 64.0) / 63.0;
    float3 up = float3(lean.x, sqrt(saturate(1.0 - dot(lean, lean))), lean.y);
    // Y onto `up`, turning about the axis square to both.
    float3 axis = float3(up.z, 0.0, -up.x);
    float s = length(axis);
    float c = up.y;
    float3x3 leanTurn = float3x3(1, 0, 0, 0, 1, 0, 0, 0, 1);
    if (s > 1e-5)
    {
        axis /= s;
        float t = 1.0 - c;
        leanTurn = float3x3(
            t * axis.x * axis.x + c, t * axis.x * axis.y - s * axis.z, t * axis.x * axis.z + s * axis.y,
            t * axis.x * axis.y + s * axis.z, t * axis.y * axis.y + c, t * axis.y * axis.z - s * axis.x,
            t * axis.x * axis.z - s * axis.y, t * axis.y * axis.z + s * axis.x, t * axis.z * axis.z + c);
    }
    float cy = cos(yaw);
    float sy = sin(yaw);
    float3x3 spin = float3x3(cy, 0, sy, 0, 1, 0, -sy, 0, cy);
    return mul(leanTurn, spin);
}

// How far over a blade standing at `foot` lies, 0 standing to 1 flat: as deep as the ground there was pressed, up to
// a few centimetres, and standing up again over `springsBack` seconds since it was last trodden (0: never). Off the
// map of tracks round the camera, nothing lies flat.
static const float GRASS_FLAT_PRESS = 0.03;
float grassTrodden(Frame* frame, float2 foot, float springsBack)
{
    float2 track = trackAt(frame, foot);
    if (track.x <= 1e-4)
    {
        return 0.0;
    }
    float age = max(frame->time - track.y, 0.0);
    float standing = springsBack > 0.0 ? smoothstep(0.0, springsBack, age) : 0.0;
    return saturate(track.x / GRASS_FLAT_PRESS) * (1.0 - standing);
}

// `v` of a blade turned `over` (0 to 1) of the way down about its foot, towards its own +z.
float3 grassLaid(float3 v, float over)
{
    // Not quite flat, so it still reads as a blade lying on the ground rather than in it.
    float angle = over * 1.4;
    float c = cos(angle);
    float s = sin(angle);
    return float3(v.x, v.y * c - v.z * s, v.y * s + v.z * c);
}

// Vertex `vertexId` of blade `instanceId` of draw `push.list`.
[shader("vertex")]
VertexOut grassVertex(uint vertexId : SV_VertexID, uint instanceId : SV_InstanceID)
{
    Frame* frame = push.frame;
    GrassDraw draw = buffers()->grassDraws[push.list];
    GrassJob job = buffers()->grassJobs[draw.job];
    GrassBlade blade = buffers()->grassBlades[draw.offset + instanceId];
    Mesh mesh = buffers()->meshes[job.mesh];
    Vertex v = buffers()->vertices[mesh.vertexOffset + vertexId];
    Material material = buffers()->materials[job.material];
    float scale = lerp(job.scale.x, job.scale.y, float((blade.packed >> 10) & 0xFF) / 255.0);
    float3x3 turn = grassTurn(blade.packed);
    float over = grassTrodden(frame, blade.position.xz, job.springsBack);
    float3 normal = normalize(mul(turn, grassLaid(v.normal, over)));
    float3 world = blade.position + mul(turn, grassLaid(v.position * scale, over));
    float height = max((mesh.center.y + mesh.radius) * scale, 0.05);
    // Lying flat, it no longer catches the wind.
    float sway = 1.0 - over;
    float3 now = world + windOffset(frame, material, blade.position, height, world, normal, v.wind, frame->time) * sway;
    float earlier = frame->time - frame->deltaTime;
    float3 before = world + windOffset(frame, material, blade.position, height, world, normal, v.wind, earlier) * sway;
    Terrain terrain = buffers()->terrains[job.terrain];

    VertexOut o;
    o.position = transform(frame->viewProj, float4(now, 1.0));
    o.worldPosition = now;
    o.normal = normal;
    o.tangent = float4(mul(turn, grassLaid(v.tangent.xyz, over)), v.tangent.w);
    o.uv = v.uv;
    o.material = job.material;
    o.lod = 0;
    // The terrain's own instance stands in for a blade's, for what the fragment shaders read of one.
    o.instance = terrain.anchor;
    o.fade = 0;
    o.cluster = 0;
    o.currentClip = transform(frame->unjitteredViewProj, float4(now, 1.0));
    o.previousClip = transform(frame->prevViewProj, float4(before, 1.0));
    o.moved = 0.0;
    o.hidden = 0.0;
    return o;
}
