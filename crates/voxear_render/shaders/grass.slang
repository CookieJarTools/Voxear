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
#include "block_quads.slang"

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
    // Counted from the level's middle (src/grass.rs), hashed by its place in a square of 65536 cells round it: each
    // blade where it is however the world's middle moves, as the rest of a scene's patterns repeat.
    int2 cell = ((job.firstCell + int2(id.xy) + 32768) & 0xFFFF) - 32768;
    uint seed = job.terrain * 64 + job.kind;
    float2 at = job.corner + (float2(id.xy) + float2(grassHash(cell, seed, 0), grassHash(cell, seed, 1))) * job.cellSize;
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

// A job's `terrain` with this bit is a world of blocks' ground cover, the world's place among the view's under it
// (src/grass.rs, BLOCK_COVER).
static const uint GRASS_BLOCK_COVER = 0x80000000u;
// How many workgroups share each chunk's faces looking up (src/grass.rs, COVER_SLICES).
static const uint GRASS_COVER_SLICES = 8;
// A run of a chunk's blocks of parts' words, two quads' a face, has this bit over its length (src/grass.rs,
// COVER_PARTS).
static const uint GRASS_COVER_PARTS = 0x80000000u;
// How much of a block a face of a block of parts looking up must cover to grow plants, in 256ths: a slab's top does,
// a fence's post does not.
static const uint GRASS_PART_LEAST = 64;
// How many draws a view's grass has room for (src/grass.rs, MAX_GRASS_DRAWS): its ground cover's words follow them.
static const uint GRASS_MAX_DRAWS = 256;

// The words of a view's ground cover (src/grass.rs, COVER_AT): each job's eight words of the surfaces it grows on,
// then its chunks' runs of faces looking up, a run's first quad in its world's pool and how many.
uint* grassCoverWords()
{
    return (uint*)(buffers()->grassDraws + GRASS_MAX_DRAWS);
}

// A world of blocks' ground cover (src/grass.rs, ground_cover_jobs): workgroup x is one of the job's chunks, y which
// share of that chunk's faces looking up it takes, a face in every GRASS_COVER_SLICES. Each face whose surface grows
// the job's plant grows them on each block it covers, as many as the job's density says on average, each where a
// hash of its block puts it: the same plants in the same places every frame and on every machine. Faces drawn one
// block each, as those that join their neighbours' pictures or that another block's picture creeps over, are one
// block whatever their size says, and a face drawn over another to show what creeps over it grows nothing.
[shader("compute")]
[numthreads(64, 1, 1)]
void groundCoverScatter(uint3 group : SV_GroupID, uint3 thread : SV_GroupThreadID)
{
    GrassJob job = buffers()->grassJobs[push.list];
    uint runs = uint(job.firstCell.y);
    if (group.x >= runs)
    {
        return;
    }
    uint* words = grassCoverWords() + uint(job.firstCell.x);
    uint2 run = uint2(words[8 + group.x * 2], words[8 + group.x * 2 + 1]);
    if ((run.y & GRASS_COVER_PARTS) != 0)
    {
        groundCoverParts(job, words, run, group.y, thread.x);
        return;
    }
    BlockWorld world = buffers()->blockWorlds[job.terrain & 0xFF];
    Frame* frame = push.frame;
    Mesh mesh = buffers()->meshes[job.mesh];
    for (uint index = group.y; index < run.y; index += GRASS_COVER_SLICES)
    {
        uint2 quad = world.quads[run.x + index];
        uint surface = quad.y & 0xFF;
        if (((words[surface >> 5] >> (surface & 31)) & 1) == 0)
        {
            continue;
        }
        uint first = quad.x;
        int4 chunk = world.origins[quad.y >> 16];
        if (chunk.w != 0)
        {
            continue;
        }
        // Looking up, a face runs along z first and x second (block_quads.slang, BLOCK_AXES).
        uint2 size = uint2(((first >> 15) & 31) + 1, ((first >> 20) & 31) + 1);
        if (world.surfaceCount > 0)
        {
            TerrainSurface drawn = world.surfaces[min(surface, world.surfaceCount - 1)];
            if ((drawn.flags & TERRAIN_SURFACE_MIXED) != 0)
            {
                // A quad more over a face drawn already (voxear_terrain::blocks::mesh::mixing's `over`).
                if (((first >> 23) & 1) != 0)
                {
                    continue;
                }
                size = uint2(1, 1);
            }
            else if ((buffers()->materials[drawn.material].flags & MATERIAL_CONNECTED) != 0)
            {
                size = uint2(1, 1);
            }
        }
        int3 at = chunk.xyz * 32 + int3(first & 31, (first >> 5) & 31, (first >> 10) & 31);
        // Whole blocks, its top a block up: z along it first, then x.
        groundCoverGrow(job, frame, mesh, world, at, uint4(0, 16, 0, 16), 16, size, thread.x);
    }
}

// Plants of `job` on a face looking up whose first block is `at`, `size` blocks along z and x: on each block, the
// part of it from `span.x` to `span.y` along z and `span.z` to `span.w` along x, in sixteenths, at `height`
// sixteenths over its foot. Threads take its plants `thread` apart, 64 at a time.
void groundCoverGrow(GrassJob job, Frame* frame, Mesh mesh, BlockWorld world, int3 at, uint4 span, uint height, uint2 size, uint thread)
{
    float density = job.corner.x;
    uint most = uint(ceil(density));
    uint plants = size.x * size.y * most;
    for (uint each = thread; each < plants; each += 64)
    {
        uint cell = each / most;
        uint one = each % most;
        int3 block = at + int3(cell / size.x, 0, cell % size.x);
        uint number = blockNumber(block, 0x5EED + job.kind * 16 + one);
        // As many on average as its density says: the last of a block's tries grows only so often.
        if (float(number & 0xFFFF) / 65535.0 >= density - float(one))
        {
            continue;
        }
        uint placed = blockNumber(block, 0xC0DE + job.kind * 16 + one);
        float2 within = float2(placed & 0xFFFF, placed >> 16) / 65535.0;
        float2 sixteenths = float2(lerp(float(span.z), float(span.w), within.x), lerp(float(span.x), float(span.y), within.y));
        float3 foot = world.corner + (float3(block) + float3(sixteenths.x, float(height), sixteenths.y) / 16.0) * world.blockSize;
        float away = distance(foot, frame->cameraPosition);
        float thinned = float((number >> 16) & 0xFF) / 255.0;
        if (away > job.drawDistance || thinned < smoothstep(job.drawDistance * 0.8, job.drawDistance, away))
        {
            continue;
        }
        float grown = float(number >> 24) / 255.0;
        float scale = lerp(job.scale.x, job.scale.y, grown);
        if (!insidePlanes(frame->planes, foot + mesh.center * scale, mesh.radius * scale))
        {
            continue;
        }
        uint lod = 0;
        while (lod + 1 < job.lodCount && away > job.lodReach[lod])
        {
            lod++;
        }
        uint slot;
        InterlockedAdd(buffers()->grassArgs[job.draws + lod].instanceCount, 1, slot);
        if (slot >= job.capacities[lod])
        {
            continue;
        }
        // Turned by quarter turns, as the blocks are, standing upright.
        uint turn = ((placed >> 3) & 3) * 256;
        GrassBlade blade;
        blade.position = foot;
        blade.packed = turn | uint(grown * 255.0) << 10 | 64u << 18 | 64u << 25;
        buffers()->grassBlades[job.offsets[lod] + slot] = blade;
    }
}

// Ground cover on the faces looking up of a chunk's blocks of parts (`run`, its words, two quads' a face;
// voxear_terrain::blocks::mesh::PartQuad): a slab's top and the like, still and covering enough of its block.
void groundCoverParts(GrassJob job, uint* words, uint2 run, uint slice, uint thread)
{
    BlockWorld world = buffers()->blockWorlds[job.terrain & 0xFF];
    Frame* frame = push.frame;
    Mesh mesh = buffers()->meshes[job.mesh];
    uint faces = (run.y & ~GRASS_COVER_PARTS) / 2;
    for (uint index = slice; index < faces; index += GRASS_COVER_SLICES)
    {
        uint2 head = world.quads[run.x + index * 2];
        uint2 tail = world.quads[run.x + index * 2 + 1];
        uint surface = head.y & 0xFF;
        bool moves = ((head.x >> 23) & 1) != 0;
        if (((head.x >> 15) & 7) != 2 || moves || ((words[surface >> 5] >> (surface & 31)) & 1) == 0)
        {
            continue;
        }
        int4 chunk = world.origins[head.y >> 16];
        if (chunk.w != 0)
        {
            continue;
        }
        // Along z and then x, in sixteenths of its block, as partPoint reads it.
        uint4 span = uint4(head.x >> 28, tail.x & 31, (head.y >> 8) & 15, (tail.x >> 5) & 31);
        if ((span.y - min(span.x, span.y)) * (span.w - min(span.z, span.w)) < GRASS_PART_LEAST)
        {
            continue;
        }
        int3 at = chunk.xyz * 32 + int3(head.x & 31, (head.x >> 5) & 31, (head.x >> 10) & 31);
        groundCoverGrow(job, frame, mesh, world, at, span, (head.x >> 18) & 31, uint2(1, 1), thread);
    }
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
    // Ground cover's plants are as other meshes: their coarser LODs are of larger cubes, which show.
    float coarser = (job.terrain & GRASS_BLOCK_COVER) != 0 ? 1.0 : 2.0;
    uint pick = 0;
    for (uint each = 1; each < mesh.lodCount; each++)
    {
        if (mesh.lods[each].error * size * frame->lodScale > frame->lodThreshold * coarser * max(near, 1e-3))
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
    float3 now = world + windOffset(frame, material, blade.position, height, scale, world, normal, v.wind, frame->time) * sway;
    float earlier = frame->time - frame->deltaTime;
    float3 before = world + windOffset(frame, material, blade.position, height, scale, world, normal, v.wind, earlier) * sway;

    VertexOut o;
    o.position = transform(frame->viewProj, float4(now, 1.0));
    o.worldPosition = now;
    o.cut = cutAt(push.frame, o.worldPosition);
    o.normal = normal;
    o.tangent = float4(mul(turn, grassLaid(v.tangent.xyz, over)), v.tangent.w);
    o.uv = v.uv;
    o.material = job.material;
    o.lod = 0;
    // The terrain's own instance, or the world of blocks', stands in for a blade's, for what the fragment shaders read
    // of one.
    bool blocks = (job.terrain & GRASS_BLOCK_COVER) != 0;
    o.instance = blocks ? buffers()->blockWorlds[job.terrain & 0xFF].anchor : buffers()->terrains[job.terrain].anchor;
    o.fade = 0;
    o.cluster = 0;
    o.currentClip = transform(frame->unjitteredViewProj, float4(now, 1.0));
    o.previousClip = transform(frame->prevViewProj, float4(before, 1.0));
    o.moved = now - world;
    o.hidden = 0.0;
    o.lightmapUv = 0.0;
    return o;
}
