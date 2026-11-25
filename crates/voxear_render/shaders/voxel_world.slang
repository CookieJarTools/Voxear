// An endless Voxel Volume's ground made on the GPU (src/voxel_volume/endless.rs): its world is a window of sector
// columns round the camera, each column four metres a side at 16 voxels a metre and as tall as the volume, kept in
// a slot of the window by where it is, wrapping. As the camera moves, the columns coming into the window are made,
// the nearest first and a few each frame, over the columns that left.
//
// Each column made goes through five passes, as the small-voxels research's generator did:
//
// 1. voxelWorldColumns: the ground's height in each of its voxel columns, with a border of four, and its climate.
// 2. voxelWorldTops: what the ground is at each column, with a border of two (grass, rock, sand, snow or gravel),
//    and its grass.
// 3. voxelWorldCandidates: each column of bricks lists the bricks the ground, grass and flowers could reach.
// 4. voxelWorldBricks: one group a listed brick, a thread a voxel, works out each voxel, and keeps the brick in its
//    job's own list if anything is in it, its materials in the column's slot in the order of its bits.
// 5. voxelWorldSectors: one group a sector of the column puts its bricks in the slot's room for bricks in the order
//    of its bits, and writes the sector's entry: where it is in the world, its bits and where its bricks start.
//
// The ground is the one a Voxel Volume's own ground is (src/voxel_volume/ground.rs), to the voxel: the same hashes,
// noise and rules, worked out from the voxel's place in the world. Or, for a volume made by a world generator, the
// generator's height, climate and rock, its graph run a voxel column at a time (generator.slang): grass where the
// ground would grow it, rock on steep ground and where the generator says, sand along the shore and in hot dry
// places, snow where it is below freezing, as a world of blocks with no biomes of its own lays them.

#include "generator.slang"

static const uint MAX_JOBS = 16;
static const uint ENTRY_WORDS = 28;
static const uint BRICK_WORDS = 20;
static const uint EMPTY = 0xFFFFFFFFu;
static const uint ROW = 1024;
// Each job's counters: bricks kept, material bytes taken, bricks placed, bricks that did not fit.
static const uint COUNTERS = 4;

struct WorldJob
{
    // The column's place in the world, in sectors along X and Z, and its slot in the window.
    int2 column;
    uint slot;
    uint pad;
};

struct WorldMake
{
    uint* table;
    uint* bricks;
    uint* materials;
    uint* palette;
    int* heights;
    uint* columns;
    uint* slots;
    uint* loose;
    uint* counters;
    uint* candidates;
    // The generator's program and what the columns pass found of each column's climate, where there is one.
    uint* program;
    uint* grounds;
    float height;
    float hills;
    float hillSpacing;
    float grass;
    float grassHeight;
    float flowers;
    uint seed;
    float voxel;
    // How many columns the window is a side, and how many sectors tall a column is.
    uint window;
    uint tall;
    // How many bricks and material bytes a column's slot has room for.
    uint brickRoom;
    uint materialRoom;
    uint jobCount;
    uint candidateRoom;
    // How many words the program has (none for the volume's own ground), the slots of its height, warmth, wetness
    // and rock, the generator's height at the volume's floor, where the volume's corner is in its world, and its sea.
    uint programWords;
    uint4 outs;
    float floor;
    float atX;
    float atZ;
    float seaLevel;
    uint pad0;
    WorldJob jobs[MAX_JOBS];
};

struct WorldPush
{
    WorldMake* make;
};

[[vk::push_constant]] ConstantBuffer<WorldPush> push;

uint hashOf(int x, int y, int z, uint seed)
{
    uint h = uint(x) * 0x8DA6B343u ^ uint(y) * 0xD8163841u ^ uint(z) * 0xCB1AB31Fu ^ seed * 0x9E3779B9u;
    h ^= h >> 16;
    h *= 0x7FEB352Du;
    h ^= h >> 15;
    h *= 0x846CA68Bu;
    h ^= h >> 16;
    return h;
}

float unit(uint h)
{
    return float(h >> 8) / 16777216.0;
}

float valueNoise(float x, float z, uint seed)
{
    int ix = int(floor(x));
    int iz = int(floor(z));
    float fx = x - floor(x);
    float fz = z - floor(z);
    float sx = fx * fx * (3.0 - 2.0 * fx);
    float sz = fz * fz * (3.0 - 2.0 * fz);
    float a = unit(hashOf(ix, 0, iz, seed));
    float b = unit(hashOf(ix + 1, 0, iz, seed));
    float c = unit(hashOf(ix, 0, iz + 1, seed));
    float d = unit(hashOf(ix + 1, 0, iz + 1, seed));
    float low = a + (b - a) * sx;
    float high = c + (d - c) * sx;
    return low + (high - low) * sz;
}

float fbm(float x, float z, uint seed)
{
    float sum = 0.0;
    float scale = 1.0;
    float weight = 0.5;
    for (uint octave = 0; octave < 4; octave++)
    {
        sum += valueNoise(x * scale, z * scale, seed + octave) * weight;
        scale *= 2.0;
        weight *= 0.5;
    }
    return sum / 0.9375;
}

// The ground's height at a voxel column, in voxels over the volume's floor.
int heightAt(WorldMake* make, int x, int z)
{
    float v = make->voxel;
    float spacing = max(make->hillSpacing, 0.5);
    float metres = make->height + make->hills * fbm((float(x) + 0.5) * v / spacing, (float(z) + 0.5) * v / spacing, make->seed);
    return int(floor(metres / v));
}

// Each column's height is kept with a border of four round the column made, so its slope and the flowers from the
// columns beside it are found; what it is, and its grass, with a border of two.
static const int BORDER = 4;
static const int SIDE = 64 + 2 * BORDER;
static const int TOP_BORDER = 2;
static const int TOPS = 64 + 2 * TOP_BORDER;

// What the ground is at a column, as tops pack it.
static const uint GROUND_GRASS = 0;
static const uint GROUND_ROCK = 1;
static const uint GROUND_SAND = 2;
static const uint GROUND_SNOW = 3;
static const uint GROUND_GRAVEL = 4;

int heightOf(WorldMake* make, uint job, int x, int z)
{
    return make->heights[job * SIDE * SIDE + uint((z + BORDER) * SIDE + x + BORDER)];
}

uint columnOf(WorldMake* make, uint job, int x, int z)
{
    return make->columns[job * TOPS * TOPS + uint((z + TOP_BORDER) * TOPS + x + TOP_BORDER)];
}

uint topOf(WorldMake* make, uint job, int x, int z)
{
    return (columnOf(make, job, x, z) >> 16) & 255u;
}

// The ground's height and climate at a voxel column from the generator, at its middle in the generator's world.
Generated generatedAt(WorldMake* make, int x, int z)
{
    float v = make->voxel;
    float gx = (float(x) + 0.5) * v + make->atX;
    float gz = (float(z) + 0.5) * v + make->atZ;
    return runGenerator(make->program, make->programWords, make->outs, gx, gz);
}

// What the ground is at a column of the generator's: its height and how steep it is round it, and its climate.
uint groundOf(WorldMake* make, uint job, int x, int z)
{
    int h = heightOf(make, job, x, z);
    int across = max(abs(heightOf(make, job, x + 2, z) - heightOf(make, job, x - 2, z)), abs(heightOf(make, job, x, z + 2) - heightOf(make, job, x, z - 2)));
    float slope = float(across) / 4.0;
    uint climate = make->grounds[job * SIDE * SIDE + uint((z + BORDER) * SIDE + x + BORDER)];
    float warmth = float(climate & 255u) - 128.0;
    float wetness = float((climate >> 8) & 255u) / 255.0;
    bool rock = ((climate >> 16) & 1u) != 0;
    float above = float(h) * make->voxel + make->floor - make->seaLevel;
    if (rock || slope > 1.0)
    {
        return GROUND_ROCK;
    }
    if (above < -0.5)
    {
        return GROUND_GRAVEL;
    }
    if (above < 1.0)
    {
        return GROUND_SAND;
    }
    if (warmth < 0.0)
    {
        return GROUND_SNOW;
    }
    if (wetness < 0.12 && warmth > 18.0)
    {
        return GROUND_SAND;
    }
    return GROUND_GRASS;
}

// The voxel of ground `top` is at `p`, `under` voxels under its column's top: its shades in a pattern of two
// voxels across, as a block's face has a pattern. Grass and its dirt are the volume's own ground's colours.
uint groundVoxel(WorldMake* make, uint top, int under, int3 p)
{
    float grain = unit(hashOf(p.x >> 1, p.y >> 1, p.z >> 1, make->seed + 31));
    switch (top)
    {
    case GROUND_ROCK:
        return grain < 0.5 ? 15u : (grain < 0.8 ? 14u : 16u);
    case GROUND_SAND:
        return grain < 0.65 ? 17u : 18u;
    case GROUND_SNOW:
        return under <= 2 ? (grain < 0.75 ? 19u : 20u) : 15u;
    case GROUND_GRAVEL:
        return grain < 0.5 ? 21u : 22u;
    default:
        return under == 0 ? 1u : 2u;
    }
}

// The voxel of a flower at `p` (in the world's voxels), or 0: each flower in a cell half a metre across, as
// VoxelGround::flowers_into places them; a voxel looks in its own cell and those round it.
uint flowerAt(WorldMake* make, uint job, int3 p)
{
    if (make->flowers <= 0.0)
    {
        return 0;
    }
    float v = make->voxel;
    float cell = 0.5;
    uint seed = make->seed + 13;
    int2 home = int2(floor(float2(p.xz) * v / cell));
    uint found = 0;
    for (int dz = -1; dz <= 1; dz++)
    {
        for (int dx = -1; dx <= 1; dx++)
        {
            int cx = home.x + dx;
            int cz = home.y + dz;
            float patch = fbm(float(cx) * cell / 6.0, float(cz) * cell / 6.0, seed);
            float chance = make->flowers * (0.3 + 1.4 * max(patch - 0.4, 0.0));
            if (unit(hashOf(cx, 3, cz, seed)) > chance)
            {
                continue;
            }
            float ax = (float(cx) + unit(hashOf(cx, 1, cz, seed))) * cell;
            float az = (float(cz) + unit(hashOf(cx, 2, cz, seed))) * cell;
            int x = int(floor(ax / v));
            int z = int(floor(az / v));
            // No flower reaches further than two voxels from its stem, which stands on its column's grass.
            int2 foot = int2(x, z) - make->jobs[job].column * 64;
            if (abs(p.x - x) > 2 || abs(p.z - z) > 2 || topOf(make, job, foot.x, foot.y) != GROUND_GRASS)
            {
                continue;
            }
            int base = heightOf(make, job, foot.x, foot.y);
            int stem = int(max((0.3 + unit(hashOf(cx, 5, cz, seed)) * 0.4) / v, 2.0));
            int top = base + stem;
            int ox = p.x - x;
            int oz = p.z - z;
            int y = p.y;
            uint m = 0;
            if (ox == 0 && oz == 0 && y > base && y <= top)
            {
                m = 12;
            }
            uint kind = uint(unit(hashOf(cx, 4, cz, seed)) * 4.0);
            if (kind == 0)
            {
                int r2 = ox * ox + oz * oz;
                if ((y == top + 1 || y == top + 2) && r2 <= 8 && (r2 < 5 || unit(hashOf(p.x, top, p.z, seed)) < 0.6))
                {
                    m = 8;
                }
                if (ox == 0 && oz == 0 && y == top + 3)
                {
                    m = 13;
                }
            }
            else if (kind == 1)
            {
                if (ox >= 0 && ox <= 1 && oz >= 0 && oz <= 1 && y >= top - 4 && y <= top + 2 && unit(hashOf(p.x, y, p.z, seed)) < 0.85)
                {
                    m = 9;
                }
            }
            else if (kind == 2)
            {
                if (abs(ox) <= 1 && abs(oz) <= 1 && y > top && y <= top + 3)
                {
                    m = 10;
                }
            }
            else
            {
                if (abs(ox) + abs(oz) <= 1 && y == top + 1)
                {
                    m = 11;
                }
                if (ox == 0 && oz == 0 && y == top + 2)
                {
                    m = 13;
                }
            }
            // A later flower is set over an earlier one, as the CPU sets them in order.
            if (m != 0)
            {
                found = m;
            }
        }
    }
    return found;
}


int lowOf(WorldMake* make, uint job, int x, int z)
{
    int h = heightOf(make, job, x, z);
    int around = min(min(heightOf(make, job, x, z - 1), heightOf(make, job, x, z + 1)), min(heightOf(make, job, x - 1, z), heightOf(make, job, x + 1, z)));
    return min(around, h) - 1;
}

[shader("compute")]
[numthreads(64, 1, 1)]
void voxelWorldColumns(uint3 id: SV_DispatchThreadID)
{
    WorldMake* make = push.make;
    uint job = id.y;
    if (job >= make->jobCount || id.x >= uint(SIDE * SIDE))
    {
        return;
    }
    int2 corner = make->jobs[job].column * 64;
    int x = corner.x + int(id.x % SIDE) - BORDER;
    int z = corner.y + int(id.x / SIDE) - BORDER;
    if (make->programWords == 0)
    {
        make->heights[job * SIDE * SIDE + id.x] = heightAt(make, x, z);
        make->grounds[job * SIDE * SIDE + id.x] = 128u + (128u << 8);
        return;
    }
    Generated made = generatedAt(make, x, z);
    make->heights[job * SIDE * SIDE + id.x] = int(floor((made.height - make->floor) / make->voxel));
    uint warmth = uint(clamp(round(made.warmth) + 128.0, 0.0, 255.0));
    uint wetness = uint(clamp(made.wetness * 255.0, 0.0, 255.0));
    uint rock = made.rock > 0.5 ? 1u : 0u;
    make->grounds[job * SIDE * SIDE + id.x] = warmth | (wetness << 8) | (rock << 16);
}

[shader("compute")]
[numthreads(64, 1, 1)]
void voxelWorldTops(uint3 id: SV_DispatchThreadID)
{
    WorldMake* make = push.make;
    uint job = id.y;
    if (job >= make->jobCount || id.x >= uint(TOPS * TOPS))
    {
        return;
    }
    int lx = int(id.x % TOPS) - TOP_BORDER;
    int lz = int(id.x / TOPS) - TOP_BORDER;
    int2 corner = make->jobs[job].column * 64;
    int x = corner.x + lx;
    int z = corner.y + lz;
    uint top = make->programWords == 0 ? GROUND_GRASS : groundOf(make, job, lx, lz);
    uint seed = make->seed + 21;
    uint r = hashOf(x, 0, z, seed);
    uint blade = 0;
    if (top == GROUND_GRASS && unit(r) < make->grass)
    {
        float tallest = max(max(make->grassHeight, 0.0) / make->voxel, 1.0);
        blade = 1 + uint(unit(hashOf(x, 1, z, seed)) * tallest);
    }
    make->columns[job * TOPS * TOPS + id.x] = blade | ((3 + (r & 3u)) << 8) | (top << 16);
}

uint packCandidate(uint job, uint bx, uint bz, uint row)
{
    return job | (bx << 4) | (bz << 7) | (row << 10);
}

[shader("compute")]
[numthreads(64, 1, 1)]
void voxelWorldCandidates(uint3 id: SV_DispatchThreadID)
{
    WorldMake* make = push.make;
    uint job = id.y;
    if (job >= make->jobCount || id.x >= 64)
    {
        return;
    }
    int bx = int(id.x % 8);
    int bz = int(id.x / 8);
    int lowest = 0x7FFFFFFF;
    int highest = -0x7FFFFFFF;
    for (int z = 0; z < 8; z++)
    {
        for (int x = 0; x < 8; x++)
        {
            int cx = bx * 8 + x;
            int cz = bz * 8 + z;
            lowest = min(lowest, lowOf(make, job, cx, cz));
            highest = max(highest, heightOf(make, job, cx, cz) + int(columnOf(make, job, cx, cz) & 255u));
        }
    }
    // Flowers stand over the grass: their stems and heads, at most.
    if (make->flowers > 0.0)
    {
        highest += int(0.7 / make->voxel) + 4;
    }
    int rows = int(make->tall) * 8;
    for (int row = max(lowest >> 3, 0); row <= min(highest >> 3, rows - 1); row++)
    {
        uint at;
        InterlockedAdd(make->counters[MAX_JOBS * COUNTERS], 1u, at);
        if (at < make->candidateRoom)
        {
            make->candidates[at] = packCandidate(job, uint(bx), uint(bz), uint(row));
        }
    }
}

[shader("compute")]
[numthreads(1, 1, 1)]
void voxelWorldArguments(uint3 id: SV_DispatchThreadID)
{
    WorldMake* make = push.make;
    uint count = min(make->counters[MAX_JOBS * COUNTERS], make->candidateRoom);
    make->counters[MAX_JOBS * COUNTERS + 1] = ROW;
    make->counters[MAX_JOBS * COUNTERS + 2] = (count + ROW - 1) / ROW;
    make->counters[MAX_JOBS * COUNTERS + 3] = 1;
}

groupshared uint sharedMask[16];
groupshared uint sharedGroups[2];
groupshared uint sharedSum[4];
groupshared uint sharedSlot;
groupshared uint sharedOffset;
groupshared uint sharedCount;
groupshared uint sharedKept;

[shader("compute")]
[numthreads(512, 1, 1)]
void voxelWorldBricks(uint3 group: SV_GroupID, uint index: SV_GroupIndex)
{
    WorldMake* make = push.make;
    uint candidate = group.y * ROW + group.x;
    if (candidate >= min(make->counters[MAX_JOBS * COUNTERS], make->candidateRoom))
    {
        return;
    }
    if (index < 16)
    {
        sharedMask[index] = 0;
    }
    if (index < 2)
    {
        sharedGroups[index] = 0;
    }
    if (index < 4)
    {
        sharedSum[index] = 0;
    }
    GroupMemoryBarrierWithGroupSync();
    uint packed = make->candidates[candidate];
    uint job = packed & 15u;
    int bx = int((packed >> 4) & 7u);
    int bz = int((packed >> 7) & 7u);
    int row = int(packed >> 10);
    int3 l = int3(index & 7u, (index >> 3) & 7u, index >> 6);
    int px = bx * 8 + l.x;
    int pz = bz * 8 + l.z;
    int y = row * 8 + l.y;
    int2 corner = make->jobs[job].column * 64;
    int tall = int(make->tall) * 64;
    int h = min(heightOf(make, job, px, pz), tall - 1);
    uint column = columnOf(make, job, px, pz);
    int blade = int(column & 255u);
    uint m = 0;
    if (h >= 0 && y <= h && y >= max(lowOf(make, job, px, pz), 0))
    {
        m = groundVoxel(make, (column >> 16) & 255u, h - y, int3(corner.x + px, y, corner.y + pz));
    }
    else if (y > h)
    {
        // Flowers over the grass, as they are set after it.
        m = flowerAt(make, job, int3(corner.x + px, y, corner.y + pz));
        if (m == 0 && h >= 0 && y <= min(h + blade, tall - 1))
        {
            m = (y == h + blade && float(blade) * make->voxel > 0.2) ? 7u : (column >> 8) & 255u;
        }
    }
    if (m != 0)
    {
        InterlockedOr(sharedMask[index >> 5], 1u << (index & 31u));
        uint g = uint((l.x >> 1) + 4 * ((l.y >> 1) + 4 * (l.z >> 1)));
        InterlockedOr(sharedGroups[g >> 5], 1u << (g & 31u));
        uint colour = make->palette[m - 1];
        InterlockedAdd(sharedSum[0], colour & 255u);
        InterlockedAdd(sharedSum[1], (colour >> 8) & 255u);
        InterlockedAdd(sharedSum[2], (colour >> 16) & 255u);
        InterlockedAdd(sharedSum[3], 1u);
    }
    GroupMemoryBarrierWithGroupSync();
    if (index == 0)
    {
        uint count = 0;
        for (uint w = 0; w < 16; w++)
        {
            count += countbits(sharedMask[w]);
        }
        sharedCount = count;
        sharedKept = 0;
        if (count > 0)
        {
            uint* counters = make->counters + job * COUNTERS;
            InterlockedAdd(counters[0], 1u, sharedSlot);
            InterlockedAdd(counters[1], count, sharedOffset);
            if (sharedSlot < make->brickRoom && sharedOffset + count <= make->materialRoom)
            {
                sharedKept = 1;
            }
            else
            {
                InterlockedAdd(counters[3], 1u);
            }
        }
    }
    GroupMemoryBarrierWithGroupSync();
    if (sharedKept == 0)
    {
        return;
    }
    uint slot = make->jobs[job].slot;
    uint materialBase = slot * make->materialRoom;
    if (m != 0)
    {
        uint rank = countbits(sharedMask[index >> 5] & ((1u << (index & 31u)) - 1u));
        for (uint w = 0; w < (index >> 5); w++)
        {
            rank += countbits(sharedMask[w]);
        }
        uint at = materialBase + sharedOffset + rank;
        InterlockedOr(make->materials[at >> 2], (m - 1) << ((at & 3u) * 8u));
    }
    uint* record = make->loose + (job * make->brickRoom + sharedSlot) * BRICK_WORDS;
    if (index < 16)
    {
        record[index] = sharedMask[index];
    }
    else if (index < 18)
    {
        record[index] = sharedGroups[index - 16];
    }
    else if (index == 18)
    {
        record[18] = materialBase + sharedOffset;
    }
    else if (index == 19)
    {
        uint3 avg = uint3(sharedSum[0], sharedSum[1], sharedSum[2]) / max(sharedSum[3], 1u);
        if (sharedSum[3] == 0)
        {
            uint dirt = make->palette[1];
            avg = uint3(dirt & 255u, (dirt >> 8) & 255u, (dirt >> 16) & 255u);
        }
        record[19] = avg.x | (avg.y << 8) | (avg.z << 16);
    }
    else if (index == 20)
    {
        uint rows = make->tall * 8;
        make->slots[job * 64 * rows + uint(row * 64 + bz * 8 + bx)] = sharedSlot + 1;
    }
}

groupshared uint sectorMask[16];
groupshared uint sectorFirst;

[shader("compute")]
[numthreads(512, 1, 1)]
void voxelWorldSectors(uint3 group: SV_GroupID, uint index: SV_GroupIndex)
{
    WorldMake* make = push.make;
    uint sy = group.x;
    uint job = group.y;
    if (job >= make->jobCount)
    {
        return;
    }
    if (index < 16)
    {
        sectorMask[index] = 0;
    }
    GroupMemoryBarrierWithGroupSync();
    int3 l = int3(index & 7u, (index >> 3) & 7u, index >> 6);
    uint row = sy * 8 + uint(l.y);
    uint rows = make->tall * 8;
    uint kept = make->slots[job * 64 * rows + row * 64 + uint(l.z * 8 + l.x)];
    if (kept != 0)
    {
        InterlockedOr(sectorMask[index >> 5], 1u << (index & 31u));
    }
    GroupMemoryBarrierWithGroupSync();
    uint slot = make->jobs[job].slot;
    if (index == 0)
    {
        uint count = 0;
        for (uint w = 0; w < 16; w++)
        {
            count += countbits(sectorMask[w]);
        }
        sectorFirst = EMPTY;
        if (count > 0)
        {
            uint placed;
            InterlockedAdd(make->counters[job * COUNTERS + 2], count, placed);
            sectorFirst = slot * make->brickRoom + placed;
        }
    }
    GroupMemoryBarrierWithGroupSync();
    uint w = make->window;
    uint* entry = make->table + ((sy * w * w) + slot) * ENTRY_WORDS;
    int2 column = make->jobs[job].column;
    if (index == 0)
    {
        entry[0] = uint(column.x);
        entry[1] = sy;
        entry[2] = uint(column.y);
    }
    else if (index >= 3 && index < 19)
    {
        entry[index] = sectorMask[index - 3];
    }
    else if (index >= 19 && index < 27)
    {
        uint pair = index - 19;
        uint before = 0;
        for (uint k = 0; k < pair * 2; k++)
        {
            before += countbits(sectorMask[k]);
        }
        entry[index] = before | ((before + countbits(sectorMask[pair * 2])) << 16);
    }
    else if (index == 27)
    {
        entry[27] = sectorFirst;
    }
    if (kept != 0)
    {
        uint rank = countbits(sectorMask[index >> 5] & ((1u << (index & 31u)) - 1u));
        for (uint k = 0; k < (index >> 5); k++)
        {
            rank += countbits(sectorMask[k]);
        }
        uint* from = make->loose + (job * make->brickRoom + kept - 1) * BRICK_WORDS;
        uint* to = make->bricks + (sectorFirst + rank) * BRICK_WORDS;
        for (uint k = 0; k < BRICK_WORDS; k++)
        {
            to[k] = from[k];
        }
    }
}

// The generator run at places alone, for checking it against the CPU's: each of `candidateRoom` places, as the bits
// of its x and z in metres in the generator's world, read from `candidates`, and what is made there written to
// `loose` as the bits of its height, warmth, wetness and rock.
[shader("compute")]
[numthreads(64, 1, 1)]
void voxelWorldPlaces(uint3 id: SV_DispatchThreadID)
{
    WorldMake* make = push.make;
    if (id.x >= make->candidateRoom)
    {
        return;
    }
    float x = asfloat(make->candidates[id.x * 2]);
    float z = asfloat(make->candidates[id.x * 2 + 1]);
    Generated made = runGenerator(make->program, make->programWords, make->outs, x, z);
    uint* out = make->loose + id.x * 4;
    out[0] = asuint(made.height);
    out[1] = asuint(made.warmth);
    out[2] = asuint(made.wetness);
    out[3] = asuint(made.rock);
}
