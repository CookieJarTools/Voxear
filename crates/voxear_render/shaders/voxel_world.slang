// An endless Voxel Volume's ground made on the GPU (src/voxel_volume/endless.rs): its world is a window of sector
// columns round the camera, each column four metres a side at 16 voxels a metre and as tall as the volume, kept in
// a slot of the window by where it is, wrapping. As the camera moves, the columns coming into the window are made,
// the nearest first and a few each frame, over the columns that left.
//
// Each column made goes through seven passes, as the small-voxels research's generator did:
//
// 1. voxelWorldColumns: the ground's height in each of its voxel columns, with a border of four, and its climate.
// 2. voxelWorldTops: what the ground is at each column, with a border of two (grass, rock, sand, snow or gravel),
//    and its grass.
// 3. voxelWorldTrees: the trees that may reach the column, placed by hash where the ground grows grass.
// 4. voxelWorldCandidates: each column of bricks lists the bricks the ground, grass, flowers and trees could reach.
// 5. voxelWorldCoarse: a thread a listed brick passes over those a Volume leaves solid or open through and through,
//    by the Volume at the brick's corners, which with caves are most of them, and those in a tree's box where its
//    plant has no brick.
// 6. voxelWorldBricks: one group a brick left, a thread a voxel, works out each voxel, and keeps the brick in its
//    job's own list if anything is in it, its materials in the column's slot in the order of its bits; or where the
//    brick is one tree's alone, the brick of the tree's plant it is.
// 7. voxelWorldSectors: one group a sector of the column puts its bricks in the slot's room for bricks in the order
//    of its bits, and writes the sector's entry: where it is in the world, its bits and where its bricks start, and
//    which of its bricks are trees' and where the plants' bricks they are are named.
//
// The ground is the one a Voxel Volume's own ground is (src/voxel_volume/ground.rs), to the voxel: the same hashes,
// noise and rules, worked out from the voxel's place in the world. Or, for a volume made by a world generator, the
// generator's height, climate and rock, its graph run a voxel column at a time (generator.slang): grass where the
// ground would grow it, rock on steep ground and where the generator says, sand along the shore and in hot dry
// places, snow where it is below freezing, as a world of blocks with no biomes of its own lays them.
//
// A generator's Volume (overhangs, tunnels, chambers) runs here too: its columns' program with the height's, what it
// carries up each column kept beside the heights, and its cells' program once a voxel in the bricks pass, over each
// brick and a border of one, so a brick keeps only the solid voxels with open air beside them.

#include "generator.slang"

static const uint MAX_JOBS = 16;
static const uint ENTRY_WORDS = 53;
static const uint BRICK_WORDS = 20;
static const uint EMPTY = 0xFFFFFFFFu;
static const uint ROW = 1024;
// Each job's counters: bricks kept, material bytes taken, bricks placed, bricks that did not fit.
// After them: how many candidate bricks there are and the coarse pass's groups, then how many of them it leaves for
// the bricks pass and that pass's groups, then how many trees stand round each job's column, then how many bricks
// each job keeps in each of its sectors, then how many of its trees' bricks are in each.
static const uint COUNTERS = 4;
static const uint SECTOR_COUNTS = MAX_JOBS * COUNTERS + 8 + MAX_JOBS;
static const uint TREE_COUNTS = SECTOR_COUNTS + MAX_JOBS * 16;
// A brick that is a tree's alone, in a job's list of its column's bricks: the plant's brick it is, marked.
static const uint TREE_BRICK = 0x80000000u;
// How many plants' bricks a word each a brick of room names: those of a slot's trees follow its own bricks.
static const uint NAMES_A_BRICK = BRICK_WORDS;
// A brick of leaves, marked over its average colour (VOXEL_LEAFY in voxels.slang).
static const uint LEAFY = 1u << 24;
// The shared room: a page of bricks, the most pages a slot has, a page of material bytes, and the most a slot has.
static const uint PAGE_BRICKS = 64;
static const uint SLOT_PAGES = 64;
static const uint MATERIAL_PAGE = 4096;
static const uint SLOT_MATERIAL_PAGES = 128;
static const uint UNPLACED = 0xFFFFFFFFu;

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
    // The Volume's cells' program, and what each column of a job carries up to its cells, `carriedCount` numbers
    // a column in the order of the heights.
    uint* volume;
    uint* carried;
    // The trees' kinds and plants (src/voxel_volume/trees.rs), the trees placed round each job's column, and room
    // for what the column under each tree's foot carries up.
    uint* forest;
    int* trees;
    uint* treeCarried;
    // The shared room (src/voxel_volume/endless.rs): each slot's pages of bricks and of material bytes, how many of
    // each it has, the pages free (bricks', then materials'), how many are free and whether any were wanted when none
    // were, each job's materials before they are placed, and where each of a job's bricks' materials are placed.
    uint* pages;
    uint* materialPages;
    uint* slotCounts;
    uint* freePages;
    uint* pool;
    uint* staging;
    uint* placed;
    // The trees' plants as voxels, in each of their four turns (src/voxel_volume/trees.rs): how many and how many
    // voxels a block is, then for each its sectors a side, where its sectors start among these words, and the least
    // corner of its box from the least corner of its trunk's foot block; their bricks and materials are the first of
    // the shared room's.
    uint* models;
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
    // How many words the Volume's cells' program has (none without one), the slot it leaves a voxel's number in,
    // how many numbers a column carries up and the columns' program's slots for them, how far over and under a
    // column's ground the Volume may be solid or hollow in voxels, and its spans: a foot and a top a word, as
    // places among the carried numbers.
    uint volumeWords;
    uint densitySlot;
    uint carriedCount;
    uint spanCount;
    uint4 carriedSlots[2];
    uint4 spans;
    float reach;
    float depth;
    // How many kinds of tree there are, and how far the furthest reaches across from its trunk, in metres.
    uint treeKinds;
    float treeReach;
    // How many pages of bricks the pool has: the free materials' pages are listed after theirs.
    uint brickPages;
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

// The height of a column's surface, in voxels: with a Volume, its top solid voxel with open air over it near its
// ground's height, as the tops pass found it; else its ground's height.
int surfaceOf(WorldMake* make, uint job, int x, int z)
{
    int h = heightOf(make, job, x, z);
    if (!hasVolume(make))
    {
        return h;
    }
    uint climate = make->grounds[job * SIDE * SIDE + uint((z + BORDER) * SIDE + x + BORDER)];
    return h + int(climate >> 17) - 16384;
}

// Whether a column's top is bare, so nothing grows on it.
bool bareAt(WorldMake* make, uint job, int x, int z)
{
    return (columnOf(make, job, x, z) >> 24) != 0;
}

// The ground's height and climate at a voxel column from the generator, at its middle in the generator's world.
Generated generatedAt(WorldMake* make, int x, int z)
{
    float v = make->voxel;
    float gx = (float(x) + 0.5) * v + make->atX;
    float gz = (float(z) + 0.5) * v + make->atZ;
    return runGenerator(make->program, make->programWords, make->outs, gx, gz);
}

bool hasVolume(WorldMake* make)
{
    return make->volumeWords != 0;
}

uint* carriedOf(WorldMake* make, uint job, int x, int z)
{
    return make->carried + (job * SIDE * SIDE + uint((z + BORDER) * SIDE + x + BORDER)) * make->carriedCount;
}

// How far into the ground of the Volume voxel `p` of the world is, in metres (out of it where negative), its column
// carrying up `carried`.
float densityWith(WorldMake* make, int3 p, uint* carried)
{
    float v = make->voxel;
    float gx = (float(p.x) + 0.5) * v + make->atX;
    float gy = (float(p.y) + 0.5) * v + make->floor;
    float gz = (float(p.z) + 0.5) * v + make->atZ;
    float slots[GENERATOR_SLOTS];
    for (uint s = 0; s < GENERATOR_SLOTS; s++)
    {
        slots[s] = 0.0;
    }
    runSteps(make->volume, make->volumeWords, gx, gy, gz, v, carried, slots);
    return slots[min(make->densitySlot, GENERATOR_SLOTS - 1)];
}

// How far into the ground of the Volume a voxel is, in metres (out of it where negative): column (x, z) of a
// job's column, voxel `y` up.
float densityAt(WorldMake* make, uint job, int x, int y, int z)
{
    int2 corner = make->jobs[job].column * 64;
    return densityWith(make, int3(corner.x + x, y, corner.y + z), carriedOf(make, job, x, z));
}

// The generator's program run at voxel column (x, z) of the world: its height, climate and rock, and what the
// column carries up to its Volume's cells, kept in `carried`.
Generated runColumn(WorldMake* make, int x, int z, uint* carried)
{
    float v = make->voxel;
    float slots[GENERATOR_SLOTS];
    for (uint s = 0; s < GENERATOR_SLOTS; s++)
    {
        slots[s] = 0.0;
    }
    runSteps(make->program, make->programWords, (float(x) + 0.5) * v + make->atX, 0.0, (float(z) + 0.5) * v + make->atZ, 0.0, make->program, slots);
    uint4 outs = make->outs;
    Generated made;
    made.height = slots[min(outs.x, GENERATOR_SLOTS - 1)];
    made.warmth = slots[min(outs.y, GENERATOR_SLOTS - 1)];
    made.wetness = slots[min(outs.z, GENERATOR_SLOTS - 1)];
    made.rock = outs.w < GENERATOR_SLOTS ? slots[outs.w] : 0.0;
    for (uint c = 0; c < make->carriedCount; c++)
    {
        carried[c] = asuint(slots[min(make->carriedSlots[c >> 2][c & 3u], GENERATOR_SLOTS - 1)]);
    }
    return made;
}

// A height the generator made, in the volume's voxels from its floor.
int voxelsUp(WorldMake* make, float height)
{
    return int(floor((height - make->floor) / make->voxel));
}

// What the ground is where it is `h` voxels high and its height changes by `across` voxels over four, as warm, wet
// and rocky as the generator says.
uint groundKind(WorldMake* make, int h, int across, float warmth, float wetness, bool rock)
{
    float slope = float(across) / 4.0;
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

// What the ground is at a column of the generator's: its height and how steep it is round it, and its climate.
uint groundOf(WorldMake* make, uint job, int x, int z)
{
    int h = heightOf(make, job, x, z);
    int across = max(abs(heightOf(make, job, x + 2, z) - heightOf(make, job, x - 2, z)), abs(heightOf(make, job, x, z + 2) - heightOf(make, job, x, z - 2)));
    uint climate = make->grounds[job * SIDE * SIDE + uint((z + BORDER) * SIDE + x + BORDER)];
    float warmth = float(climate & 255u) - 128.0;
    float wetness = float((climate >> 8) & 255u) / 255.0;
    return groundKind(make, h, across, warmth, wetness, ((climate >> 16) & 1u) != 0);
}

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
            if (abs(p.x - x) > 2 || abs(p.z - z) > 2 || topOf(make, job, foot.x, foot.y) != GROUND_GRASS || bareAt(make, job, foot.x, foot.y))
            {
                continue;
            }
            int base = surfaceOf(make, job, foot.x, foot.y);
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
    Generated made = runColumn(make, x, z, make->carried + (job * SIDE * SIDE + id.x) * make->carriedCount);
    make->heights[job * SIDE * SIDE + id.x] = voxelsUp(make, made.height);
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
    // With a Volume, the column's surface is its top solid voxel with open air over it within four metres of its
    // ground's height, found going down through the air by half as far as the Volume says the ground is, two voxels
    // at least, and then halving the last step. Where there is ground over all of that, or none in it, the column is
    // bare: nothing grows on it, though what the ground is there still colours the Volume's ground open to the sky.
    bool bare = false;
    if (hasVolume(make))
    {
        int h = heightOf(make, job, lx, lz);
        int reach = int(min(4.0 / make->voxel, 1024.0));
        int y = h;
        // Mostly the Volume leaves the ground's height where it was: two looks find that.
        if (!(densityAt(make, job, lx, h, lz) > 0.0 && densityAt(make, job, lx, h + 1, lz) <= 0.0))
        {
            int lowest = h - reach;
            y = h + reach;
            float d = densityAt(make, job, lx, y, lz);
            bare = d > 0.0;
            bool found = false;
            while (!bare && !found && y > lowest)
            {
                int stride = clamp(int(-d * 0.5 / make->voxel), 2, y - lowest);
                float below = densityAt(make, job, lx, y - stride, lz);
                if (below > 0.0)
                {
                    int open = y;
                    int solid = y - stride;
                    while (open - solid > 1)
                    {
                        int middle = (open + solid) / 2;
                        if (densityAt(make, job, lx, middle, lz) > 0.0)
                        {
                            solid = middle;
                        }
                        else
                        {
                            open = middle;
                        }
                    }
                    y = solid;
                    found = true;
                }
                else
                {
                    y -= stride;
                    d = below;
                }
            }
            bare = bare || !found;
        }
        uint at = job * SIDE * SIDE + uint((lz + BORDER) * SIDE + lx + BORDER);
        int offset = bare ? 0 : clamp(y - h, -16000, 16000);
        make->grounds[at] = (make->grounds[at] & 0x1FFFFu) | (uint(offset + 16384) << 17);
    }
    uint seed = make->seed + 21;
    uint r = hashOf(x, 0, z, seed);
    uint blade = 0;
    if (top == GROUND_GRASS && !bare && unit(r) < make->grass)
    {
        float tallest = max(max(make->grassHeight, 0.0) / make->voxel, 1.0);
        blade = 1 + uint(unit(hashOf(x, 1, z, seed)) * tallest);
    }
    make->columns[job * TOPS * TOPS + id.x] = blade | ((3 + (r & 3u)) << 8) | (top << 16) | (bare ? 1u << 24 : 0u);
}

// Trees (src/voxel_volume/trees.rs): each kind grown as a few plants, each plant's voxels made once on the CPU in each
// of four turns. A trees pass places them round each job's column, at most one in each cell TREE_CELL metres square,
// by hash, where the ground grows grass, each with its plant's box on the bricks' grid, so the plant's bricks are the
// world's. A brick that is one tree's alone names its plant's brick; one that trees share, or a tree and the ground,
// is worked out of them all.
static const uint MAX_TREES = 32;
static const uint TREE_WORDS = 8;
static const uint TREE_VARIANTS = 4;
static const float TREE_CELL = 5.0;
static const uint TREE_KINDS_AT = 4;
static const uint TREE_PLANTS_AT = TREE_KINDS_AT + 8 * 4;
static const uint TREE_PLANT_WORDS = 8;
static const uint MOST_CARRIED_HERE = 8;
static const uint MODELS_AT = 4;
static const uint MODEL_WORDS = 8;
static const uint MODEL_SECTOR_WORDS = 25;

// How many trees stand round a job's column.
uint treeCount(WorldMake* make, uint job)
{
    return min(make->counters[MAX_JOBS * COUNTERS + 8 + job], MAX_TREES);
}

// A tree placed round a job's column: the least corner of its plant's box in the world's voxels, on the bricks' grid,
// which of the plants' turns it is, and the size of the box.
int* treeOf(WorldMake* make, uint job, uint t)
{
    return make->trees + (job * MAX_TREES + t) * TREE_WORDS;
}

// How many voxels a block of a tree is a side: a metre's.
int blockVoxels(WorldMake* make)
{
    return max(int(round(1.0 / make->voxel)), 1);
}

// Whether a tree's box reaches the box of voxels from `low` to `high` of the world.
bool treeReaches(WorldMake* make, int* tree, int3 low, int3 high)
{
    int3 least = int3(tree[0], tree[1], tree[2]);
    int3 most = least + int3(tree[5], tree[6], tree[7]) - 1;
    return all(high >= least) && all(low <= most);
}

// Whether a group of leaves with only a few of its voxels is kept: three in five, by hash (bricks::loosely_kept).
bool looselyKept(uint seed, uint group)
{
    uint h = seed * 0x9E3779B9u ^ group * 0x85EBCA6Bu;
    h ^= h >> 16;
    h *= 0x7FEB352Du;
    h ^= h >> 15;
    return (h >> 8) % 5u < 3u;
}

// The brick of a tree's plant at the brick of the world whose least voxel is `p`, among the shared room's: -1 for none.
int treeBrick(WorldMake* make, int* tree, int3 p)
{
    uint* r = make->models + MODELS_AT + uint(tree[3]) * MODEL_WORDS;
    int3 b = (p - int3(tree[0], tree[1], tree[2])) >> 3;
    int3 count = int3(int(r[0]), int(r[1]), int(r[2]));
    if (any(b < 0) || any(b >= count * 8))
    {
        return -1;
    }
    int3 s = b >> 3;
    uint* sector = make->models + r[3] + uint((s.y * count.z + s.z) * count.x + s.x) * MODEL_SECTOR_WORDS;
    if (sector[24] == EMPTY)
    {
        return -1;
    }
    int3 l = b & 7;
    uint index = uint(l.x + 8 * (l.y + 8 * l.z));
    uint word = sector[index >> 5];
    if (((word >> (index & 31u)) & 1u) == 0u)
    {
        return -1;
    }
    uint before = (sector[16 + (index >> 6)] >> (((index >> 5) & 1u) * 16u)) & 0xFFFFu;
    return int(sector[24] + before + countbits(word & ((1u << (index & 31u)) - 1u)));
}

// What a plant's brick has at its voxel `l`: a palette place and one, or nothing.
uint treeVoxel(WorldMake* make, int brick, int3 l)
{
    uint* bits = make->bricks + uint(brick) * BRICK_WORDS;
    uint index = uint(l.x + 8 * (l.y + 8 * l.z));
    if (((bits[index >> 5] >> (index & 31u)) & 1u) == 0u)
    {
        return 0;
    }
    uint before = countbits(bits[index >> 5] & ((1u << (index & 31u)) - 1u));
    for (uint k = 0; k < (index >> 5); k++)
    {
        before += countbits(bits[k]);
    }
    uint at = bits[18] + before;
    return ((make->materials[at >> 2] >> ((at & 3u) * 8u)) & 255u) + 1u;
}

// Whether the ground at voxel column `foot` of the world grows grass, and how high it is there: worked out afresh,
// as a tree's foot may be outside the job's columns. `carried` is room for what the column carries up.
bool grassAt(WorldMake* make, int2 foot, uint* carried, out int h)
{
    int tall = int(make->tall) * 64;
    if (make->programWords == 0)
    {
        h = heightAt(make, foot.x, foot.y);
        return h >= 0 && h < tall - 1;
    }
    int east = voxelsUp(make, runColumn(make, foot.x + 2, foot.y, carried).height);
    int west = voxelsUp(make, runColumn(make, foot.x - 2, foot.y, carried).height);
    int north = voxelsUp(make, runColumn(make, foot.x, foot.y + 2, carried).height);
    int south = voxelsUp(make, runColumn(make, foot.x, foot.y - 2, carried).height);
    Generated made = runColumn(make, foot.x, foot.y, carried);
    h = voxelsUp(make, made.height);
    if (h < 0 || h >= tall - 1)
    {
        return false;
    }
    int across = max(abs(east - west), abs(north - south));
    if (groundKind(make, h, across, made.warmth, made.wetness, made.rock > 0.5) != GROUND_GRASS)
    {
        return false;
    }
    // Where the Volume moved the ground, or hollowed it under the foot, nothing is planted.
    return !hasVolume(make) || (densityWith(make, int3(foot.x, h, foot.y), carried) > 0.0 && densityWith(make, int3(foot.x, h + 1, foot.y), carried) <= 0.0);
}

// Places the trees that may reach each job's column: a thread a cell round it.
[shader("compute")]
[numthreads(64, 1, 1)]
void voxelWorldTrees(uint3 id: SV_DispatchThreadID)
{
    WorldMake* make = push.make;
    uint job = id.y;
    if (job >= make->jobCount || make->treeKinds == 0)
    {
        return;
    }
    float v = make->voxel;
    int2 corner = make->jobs[job].column * 64;
    // Leaves round over a block past the plant's own, and a tree stands up to a brick aside, to keep to their grid.
    float reach = make->treeReach + 1.5;
    int2 first = int2(floor((float2(corner) * v - reach) / TREE_CELL));
    int2 last = int2(floor((float2(corner + 64) * v + reach) / TREE_CELL));
    int2 cells = last - first + 1;
    uint seed = make->seed + 91;
    uint* carried = make->treeCarried + (job * 64 + id.x) * MOST_CARRIED_HERE;
    for (uint i = id.x; i < uint(cells.x * cells.y); i += 64)
    {
        int2 cell = first + int2(int(i % uint(cells.x)), int(i / uint(cells.x)));
        float chance = unit(hashOf(cell.x, 7, cell.y, seed));
        uint kind = 0;
        while (kind < make->treeKinds && chance >= asfloat(make->forest[TREE_KINDS_AT + kind * 4]))
        {
            kind++;
        }
        if (kind == make->treeKinds)
        {
            continue;
        }
        float2 within = float2(unit(hashOf(cell.x, 8, cell.y, seed)), unit(hashOf(cell.x, 9, cell.y, seed)));
        int2 foot = int2(floor((float2(cell) + 0.15 + 0.7 * within) * TREE_CELL / v));
        int h;
        if (!grassAt(make, foot, carried, h))
        {
            continue;
        }
        uint pick = hashOf(cell.x, 10, cell.y, seed);
        uint* kindOf = make->forest + TREE_KINDS_AT + kind * 4;
        uint plantIndex = kindOf[1] + pick % max(kindOf[2], 1u);
        uint model = plantIndex * 4 + ((pick >> 8) & 3u);
        uint* r = make->models + MODELS_AT + model * MODEL_WORDS;
        int b = blockVoxels(make);
        uint t;
        InterlockedAdd(make->counters[MAX_JOBS * COUNTERS + 8 + job], 1u, t);
        if (t >= MAX_TREES)
        {
            continue;
        }
        int* tree = treeOf(make, job, t);
        // Its plant's box on the bricks' grid: the tree stands up to a brick less a voxel aside and lower, its foot in
        // the ground.
        int3 trunk = int3(foot.x - b / 2, h + 1, foot.y - b / 2);
        int3 least = (trunk + int3(int(r[4]), int(r[5]), int(r[6]))) & ~7;
        tree[0] = least.x;
        tree[1] = least.y;
        tree[2] = least.z;
        tree[3] = int(model);
        tree[4] = 0;
        tree[5] = int(r[0]) * 64;
        tree[6] = int(r[1]) * 64;
        tree[7] = int(r[2]) * 64;
    }
}

// A candidate brick's marks: where the ground, its grass or its flowers may be in it, which is kept whatever the
// coarse pass finds of a Volume; and where a tree's box reaches it.
static const uint GROWN = 1u << 31;
static const uint TREED = 1u << 30;
static const uint MARKS = GROWN | TREED;

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
    // With a Volume, from under its lowest surface to over the tallest grass on it: the bricks there are kept
    // whatever the coarse pass finds of the Volume, as the grass and flowers are not its.
    int lowSurface = 0x7FFFFFFF;
    int highGrowth = -0x7FFFFFFF;
    bool volume = hasVolume(make);
    for (int z = 0; z < 8; z++)
    {
        for (int x = 0; x < 8; x++)
        {
            int cx = bx * 8 + x;
            int cz = bz * 8 + z;
            int blade = int(columnOf(make, job, cx, cz) & 255u);
            lowest = min(lowest, lowOf(make, job, cx, cz));
            highest = max(highest, heightOf(make, job, cx, cz) + blade);
            if (volume)
            {
                int surface = surfaceOf(make, job, cx, cz);
                lowSurface = min(lowSurface, surface);
                highGrowth = max(highGrowth, surface + blade);
            }
        }
    }
    // Flowers stand over the grass: their stems and heads, at most.
    if (make->flowers > 0.0)
    {
        highest += int(0.7 / make->voxel) + 4;
        highGrowth += int(0.7 / make->voxel) + 4;
    }
    int rows = int(make->tall) * 8;
    // Without a Volume, the ground, its grass and its flowers are in these rows alone.
    if (!volume)
    {
        lowSurface = lowest + 1;
        highGrowth = highest;
    }
    // The trees whose boxes reach this column of bricks.
    int2 corner = make->jobs[job].column * 64;
    int3 low = int3(corner.x + bx * 8, 0, corner.y + bz * 8);
    int3 high = int3(low.x + 7, rows * 8 - 1, low.z + 7);
    int treeLow = 0x7FFFFFFF;
    int treeHigh = -0x7FFFFFFF;
    for (uint t = 0; t < treeCount(make, job); t++)
    {
        int* tree = treeOf(make, job, t);
        if (treeReaches(make, tree, low, high))
        {
            treeLow = min(treeLow, tree[1]);
            treeHigh = max(treeHigh, tree[1] + tree[6] - 1);
        }
    }
    lowest = min(lowest, treeLow);
    highest = max(highest, treeHigh);
    // A Volume reaches over and under its ground as far as it says, and as far as each column's spans say.
    if (volume)
    {
        highest += int(min(make->reach, float(rows * 8))) + 1;
        lowest -= int(min(make->depth, float(rows * 8))) + 1;
        for (int z = 0; z < 8; z++)
        {
            for (int x = 0; x < 8; x++)
            {
                uint* carried = carriedOf(make, job, bx * 8 + x, bz * 8 + z);
                for (uint span = 0; span < make->spanCount; span++)
                {
                    uint places = make->spans[span];
                    float foot = asfloat(carried[places & 255u]);
                    float top = asfloat(carried[(places >> 8) & 255u]);
                    if (top > foot)
                    {
                        float from = clamp((foot - make->floor) / make->voxel, -1.0, float(rows * 8));
                        float to = clamp((top - make->floor) / make->voxel, -1.0, float(rows * 8));
                        lowest = min(lowest, int(floor(from)) - 1);
                        highest = max(highest, int(ceil(to)) + 1);
                    }
                }
            }
        }
    }
    for (int row = max(lowest >> 3, 0); row <= min(highest >> 3, rows - 1); row++)
    {
        uint at;
        InterlockedAdd(make->counters[MAX_JOBS * COUNTERS], 1u, at);
        bool grown = row * 8 + 8 >= lowSurface - 1 && row * 8 - 1 <= highGrowth;
        bool treed = row * 8 + 7 >= treeLow && row * 8 <= treeHigh;
        if (at < make->candidateRoom)
        {
            make->candidates[at] = packCandidate(job, uint(bx), uint(bz), uint(row)) | (grown ? GROWN : 0u) | (treed ? TREED : 0u);
        }
    }
}

[shader("compute")]
[numthreads(1, 1, 1)]
void voxelWorldArguments(uint3 id: SV_DispatchThreadID)
{
    WorldMake* make = push.make;
    uint count = min(make->counters[MAX_JOBS * COUNTERS], make->candidateRoom);
    make->counters[MAX_JOBS * COUNTERS + 1] = (count + 63) / 64;
    make->counters[MAX_JOBS * COUNTERS + 2] = 1;
    make->counters[MAX_JOBS * COUNTERS + 3] = 1;
}

// How far into or out of the ground, in voxels, the corners of a brick and its border must all be for the brick to be
// taken as solid or open through and through. A voxel of the ten a side is at most seven voxels from a corner. The
// Volume's numbers read about as how far a place is from the ground's surface, but near caves they change up to three
// times as fast, and now and then faster: three times seven, and a little over.
static const float COARSE_EVEN = 24.0;

// Whether a candidate brick of a Volume is solid or open through and through, by the eight corners of the brick and
// its border of one: such a brick has no voxel to keep.
bool evenThrough(WorldMake* make, uint packed)
{
    uint job = packed & 15u;
    int3 brick = int3(int((packed >> 4) & 7u), int((packed & ~MARKS) >> 10), int((packed >> 7) & 7u)) * 8;
    int tall = int(make->tall) * 64;
    float even = COARSE_EVEN * make->voxel;
    bool solid = true;
    bool open = true;
    for (uint c = 0; c < 8 && (solid || open); c++)
    {
        int3 q = brick + int3(int(c & 1u), int((c >> 1) & 1u), int(c >> 2)) * 9 - 1;
        // Under the volume's bottom voxels is solid, and over its top open, as the bricks pass takes them.
        float d = q.y <= 0 ? 1.0e9 : (q.y < tall ? densityAt(make, job, q.x, q.y, q.z) : -1.0e9);
        solid = solid && d > even;
        open = open && d < -even;
    }
    return solid || open;
}

// Whether any tree's plant has a brick at a candidate brick.
bool treesIn(WorldMake* make, uint packed)
{
    if ((packed & TREED) == 0)
    {
        return false;
    }
    uint job = packed & 15u;
    int2 corner = make->jobs[job].column * 64;
    int3 low = int3(corner.x, 0, corner.y) + int3(int((packed >> 4) & 7u), int((packed & ~MARKS) >> 10), int((packed >> 7) & 7u)) * 8;
    int3 high = low + 7;
    for (uint t = 0; t < treeCount(make, job); t++)
    {
        int* tree = treeOf(make, job, t);
        if (treeReaches(make, tree, low, high) && treeBrick(make, tree, low) >= 0)
        {
            return true;
        }
    }
    return false;
}

// Passes over the candidate bricks a Volume leaves solid or open through and through, and those listed for a tree's
// box where its plant has no brick, before a group of 512 works out each one's voxels: the rest go on, in the second
// half of the candidates, to the bricks pass. Without a Volume, the ground's own bricks all go on.
[shader("compute")]
[numthreads(64, 1, 1)]
void voxelWorldCoarse(uint3 id: SV_DispatchThreadID)
{
    WorldMake* make = push.make;
    uint count = min(make->counters[MAX_JOBS * COUNTERS], make->candidateRoom);
    if (id.x >= count)
    {
        return;
    }
    uint packed = make->candidates[id.x];
    if ((packed & GROWN) == 0 && !(hasVolume(make) && !evenThrough(make, packed)) && !treesIn(make, packed))
    {
        return;
    }
    uint at;
    InterlockedAdd(make->counters[MAX_JOBS * COUNTERS + 4], 1u, at);
    make->candidates[make->candidateRoom + at] = packed;
}

[shader("compute")]
[numthreads(1, 1, 1)]
void voxelWorldBrickArguments(uint3 id: SV_DispatchThreadID)
{
    WorldMake* make = push.make;
    uint count = make->counters[MAX_JOBS * COUNTERS + 4];
    make->counters[MAX_JOBS * COUNTERS + 5] = ROW;
    make->counters[MAX_JOBS * COUNTERS + 6] = (count + ROW - 1) / ROW;
    make->counters[MAX_JOBS * COUNTERS + 7] = 1;
}

groupshared uint sharedMask[16];
groupshared uint sharedGroups[2];
// How many voxels each group has, and how many of the brick's voxels are trees': a brick mostly trees' is leaves, whose
// groups are loosely filled as its voxels are (VOXEL_LEAFY in voxels.slang).
groupshared uint sharedGroupCounts[64];
groupshared uint sharedTreeVoxels;
groupshared uint sharedSum[4];
// With a Volume: which voxels of the brick and a border of one round it are solid, ten a side.
groupshared uint sharedSolid[32];
groupshared uint sharedEven;
groupshared uint sharedSlot;
groupshared uint sharedOffset;
groupshared uint sharedCount;
groupshared uint sharedKept;
// The bricks of the trees' plants at the brick, and whether the ground has anything in it.
groupshared uint sharedTrees[MAX_TREES];
groupshared uint sharedTreeCount;
groupshared uint sharedOwn;

// How far into or out of the ground a brick's first look must find every voxel it looks at to take the whole
// brick as solid or open, in metres.
static const float VOLUME_EVEN = 0.3;

// Whether the voxel `off` from voxel `l` of the brick is solid, from what the group worked out.
bool solidNear(int3 l, int3 off)
{
    int3 r = l + off + 1;
    uint at = uint(r.x + 10 * (r.y + 10 * r.z));
    return ((sharedSolid[at >> 5] >> (at & 31u)) & 1u) != 0;
}

// The voxel the Volume makes at voxel `l` of the brick, `p` in the world's voxels, whose column's ground is `h`
// high: nothing where it is open, or solid with no open air beside it, which is never seen. What the column's ground
// is where it is open to the sky, as a top; the layers under it near the column's ground, in the sides of overhangs
// and the mouths of caves; rock deeper, in the walls of caves.
uint volumeVoxel(WorldMake* make, uint job, uint column, int h, int3 l, int3 p)
{
    if (!solidNear(l, int3(0, 0, 0)))
    {
        return 0;
    }
    bool open = !solidNear(l, int3(0, 1, 0));
    bool seen = open || !solidNear(l, int3(0, -1, 0)) || !solidNear(l, int3(1, 0, 0)) || !solidNear(l, int3(-1, 0, 0))
        || !solidNear(l, int3(0, 0, 1)) || !solidNear(l, int3(0, 0, -1));
    if (!seen)
    {
        return 0;
    }
    uint top = (column >> 16) & 255u;
    int under = surfaceOf(make, job, p.x - make->jobs[job].column.x * 64, p.z - make->jobs[job].column.y * 64) - p.y;
    // A ledge in a cave's wall or under ground standing apart is rock, open to the sky or not.
    if (under > 8 || under < -2)
    {
        return groundVoxel(make, GROUND_ROCK, under, p);
    }
    return groundVoxel(make, top, open ? 0 : max(under, 1), p);
}

[shader("compute")]
[numthreads(512, 1, 1)]
void voxelWorldBricks(uint3 group: SV_GroupID, uint index: SV_GroupIndex)
{
    WorldMake* make = push.make;
    uint candidate = group.y * ROW + group.x;
    if (candidate >= make->counters[MAX_JOBS * COUNTERS + 4])
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
    if (index < 64)
    {
        sharedGroupCounts[index] = 0;
    }
    if (index < 4)
    {
        sharedSum[index] = 0;
    }
    if (index < 32)
    {
        sharedSolid[index] = 0;
    }
    if (index == 0)
    {
        sharedEven = 0;
        sharedTreeCount = 0;
        sharedOwn = 0;
        sharedTreeVoxels = 0;
    }
    GroupMemoryBarrierWithGroupSync();
    uint packed = make->candidates[make->candidateRoom + candidate];
    uint job = packed & 15u;
    int bx = int((packed >> 4) & 7u);
    int bz = int((packed >> 7) & 7u);
    int row = int((packed & ~MARKS) >> 10);
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
    if ((packed & TREED) != 0)
    {
        int3 low = int3(corner.x + bx * 8, row * 8, corner.y + bz * 8);
        int brick = -1;
        if (index < treeCount(make, job) && treeReaches(make, treeOf(make, job, index), low, low + 7))
        {
            brick = treeBrick(make, treeOf(make, job, index), low);
        }
        if (brick >= 0)
        {
            uint at;
            InterlockedAdd(sharedTreeCount, 1u, at);
            sharedTrees[at] = uint(brick);
        }
    }
    GroupMemoryBarrierWithGroupSync();
    bool volume = hasVolume(make);
    if (volume)
    {
        // First a look at 64 voxels spread through the brick and its border, three voxels apart: where every one is
        // further into the ground than VOLUME_EVEN metres, the whole is taken as solid, and where every one is
        // further out of it, as open, without working out the thousand. The Volume's numbers read about as how far
        // a place is from the ground's surface, and a cave or an overhang is wider than the gap between the looks.
        if (index < 64)
        {
            int3 r = int3(index % 4, (index / 4) % 4, index / 16) * 3 - 1;
            int3 q = int3(bx * 8, row * 8, bz * 8) + r;
            float d = q.y <= 0 ? 1.0e9 : (q.y < tall ? densityAt(make, job, q.x, q.y, q.z) : -1.0e9);
            if (d <= VOLUME_EVEN)
            {
                InterlockedOr(sharedEven, 1u);
            }
            if (d >= -VOLUME_EVEN)
            {
                InterlockedOr(sharedEven, 2u);
            }
        }
        GroupMemoryBarrierWithGroupSync();
    }
    // 1: not all solid; 2: not all open.
    uint even = sharedEven;
    if (volume && (even & 1u) == 0)
    {
        if (index < 32)
        {
            sharedSolid[index] = 0xFFFFFFFFu;
        }
        GroupMemoryBarrierWithGroupSync();
    }
    else if (volume && (even & 2u) != 0)
    {
        // Each thread works out one or two of the thousand voxels round the brick. The volume's bottom voxels are
        // solid, so a cave that goes deeper has a floor there, and what is under them is solid too, so the underside
        // of the floor is never drawn; over the top is open.
        for (uint at = index; at < 1000; at += 512)
        {
            int3 r = int3(at % 10, (at / 10) % 10, at / 100) - 1;
            int3 q = int3(bx * 8, row * 8, bz * 8) + r;
            bool solid = q.y <= 0 || (q.y < tall && densityAt(make, job, q.x, q.y, q.z) > 0.0);
            if (solid)
            {
                InterlockedOr(sharedSolid[at >> 5], 1u << (at & 31u));
            }
        }
        GroupMemoryBarrierWithGroupSync();
    }
    if (volume)
    {
        m = volumeVoxel(make, job, column, h, l, int3(corner.x + px, y, corner.y + pz));
        int surface = min(surfaceOf(make, job, px, pz), tall - 1);
        if (m == 0 && y > surface && !solidNear(l, int3(0, 0, 0)))
        {
            m = flowerAt(make, job, int3(corner.x + px, y, corner.y + pz));
            if (m == 0 && surface >= 0 && y <= min(surface + blade, tall - 1))
            {
                m = (y == surface + blade && float(blade) * make->voxel > 0.2) ? 7u : (column >> 8) & 255u;
            }
        }
    }
    else if (h >= 0 && y <= h && y >= max(lowOf(make, job, px, pz), 0))
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
    // A brick one tree's alone is its plant's brick: the job's list names it.
    if (sharedTreeCount == 1)
    {
        if (m != 0)
        {
            sharedOwn = 1;
        }
        GroupMemoryBarrierWithGroupSync();
    }
    if (sharedTreeCount == 1 && sharedOwn == 0)
    {
        if (index == 0)
        {
            uint rows = make->tall * 8;
            make->slots[job * 64 * rows + uint(row * 64 + bz * 8 + bx)] = TREE_BRICK | sharedTrees[0];
            InterlockedAdd(make->counters[TREE_COUNTS + job * 16 + uint(row) / 8], 1u);
        }
        return;
    }
    // Else a tree's voxel where the ground leaves none, and over its grass and flowers, in their order: the plants'
    // bricks are on the world's grid.
    int ground = volume ? min(surfaceOf(make, job, px, pz), tall - 1) : h;
    for (uint t = 0; t < sharedTreeCount && (m == 0 || y > ground); t++)
    {
        uint filled = treeVoxel(make, int(sharedTrees[t]), l);
        if (filled != 0)
        {
            m = filled;
            InterlockedAdd(sharedTreeVoxels, 1u);
            break;
        }
    }
    if (m != 0)
    {
        InterlockedOr(sharedMask[index >> 5], 1u << (index & 31u));
        uint g = uint((l.x >> 1) + 4 * ((l.y >> 1) + 4 * (l.z >> 1)));
        InterlockedOr(sharedGroups[g >> 5], 1u << (g & 31u));
        InterlockedAdd(sharedGroupCounts[g], 1u);
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
            // Its materials are kept a word at a time, so they are placed a word at a time.
            uint* counters = make->counters + job * COUNTERS;
            InterlockedAdd(counters[0], 1u, sharedSlot);
            InterlockedAdd(counters[1], (count + 3u) & ~3u, sharedOffset);
            if (sharedSlot < make->brickRoom && sharedOffset + count <= make->materialRoom)
            {
                sharedKept = 1;
            }
            else
            {
                InterlockedAdd(counters[3], 1u);
                // A brick there is no room for is marked so the pack pass places nothing for it.
                if (sharedSlot < make->brickRoom)
                {
                    make->loose[(job * make->brickRoom + sharedSlot) * BRICK_WORDS + 18] = UNPLACED;
                }
            }
        }
    }
    GroupMemoryBarrierWithGroupSync();
    if (sharedKept == 0)
    {
        return;
    }
    uint materialBase = job * make->materialRoom;
    if (m != 0)
    {
        uint rank = countbits(sharedMask[index >> 5] & ((1u << (index & 31u)) - 1u));
        for (uint w = 0; w < (index >> 5); w++)
        {
            rank += countbits(sharedMask[w]);
        }
        uint at = materialBase + sharedOffset + rank;
        InterlockedOr(make->staging[at >> 2], (m - 1) << ((at & 3u) * 8u));
    }
    uint* record = make->loose + (job * make->brickRoom + sharedSlot) * BRICK_WORDS;
    if (index < 16)
    {
        record[index] = sharedMask[index];
    }
    else if (index < 18)
    {
        uint groups = sharedGroups[index - 16];
        if (sharedTreeVoxels * 2u >= sharedCount)
        {
            groups = 0;
            uint seed = hashOf(corner.x + bx * 8, row * 8, corner.y + bz * 8, make->seed + 53);
            for (uint g = 0; g < 32; g++)
            {
                uint count = sharedGroupCounts[(index - 16) * 32 + g];
                groups |= count >= 5u || (count > 0u && looselyKept(seed, (index - 16) * 32 + g)) ? 1u << g : 0u;
            }
        }
        record[index] = groups;
    }
    else if (index == 18)
    {
        // Where its materials are in the job's own, and how many: placed in the shared room by the pack pass.
        record[18] = sharedOffset | (sharedCount << 20);
    }
    else if (index == 19)
    {
        uint3 avg = uint3(sharedSum[0], sharedSum[1], sharedSum[2]) / max(sharedSum[3], 1u);
        if (sharedSum[3] == 0)
        {
            uint dirt = make->palette[1];
            avg = uint3(dirt & 255u, (dirt >> 8) & 255u, (dirt >> 16) & 255u);
        }
        record[19] = avg.x | (avg.y << 8) | (avg.z << 16) | (sharedTreeVoxels * 2u >= sharedCount && sharedCount > 0 ? LEAFY : 0u);
    }
    else if (index == 20)
    {
        uint rows = make->tall * 8;
        make->slots[job * 64 * rows + uint(row * 64 + bz * 8 + bx)] = sharedSlot + 1;
        InterlockedAdd(make->counters[SECTOR_COUNTS + job * 16 + uint(row) / 8], 1u);
    }
}

// Gives back the pages each job's slot held for the column it held before: a thread a job. A page given back is taken
// again by the pack pass after it, for this frame's columns.
[shader("compute")]
[numthreads(64, 1, 1)]
void voxelWorldFree(uint3 id: SV_DispatchThreadID)
{
    WorldMake* make = push.make;
    uint job = id.x;
    if (job >= make->jobCount)
    {
        return;
    }
    uint slot = make->jobs[job].slot;
    uint held = make->slotCounts[slot];
    for (uint i = 0; i < (held & 0xFFFFu); i++)
    {
        uint at;
        InterlockedAdd(make->pool[0], 1u, at);
        make->freePages[at] = make->pages[slot * SLOT_PAGES + i];
    }
    for (uint i = 0; i < (held >> 16); i++)
    {
        uint at;
        InterlockedAdd(make->pool[1], 1u, at);
        make->freePages[make->brickPages + at] = make->materialPages[slot * SLOT_MATERIAL_PAGES + i];
    }
    make->slotCounts[slot] = 0;
}

// A free page of bricks (`which` 0) or of materials (1), or UNPLACED where none is left, which is noted.
uint takePage(WorldMake* make, uint which)
{
    uint was;
    InterlockedAdd(make->pool[which], 0xFFFFFFFFu, was);
    if (was == 0u || was == UNPLACED)
    {
        InterlockedAdd(make->pool[which], 1u);
        InterlockedOr(make->pool[2], 1u << which);
        return UNPLACED;
    }
    return make->freePages[(which == 0u ? 0u : make->brickPages) + was - 1u];
}

// Takes the pages each job's column needs from the free ones, and places each of its bricks' materials in them, a
// brick's never across the end of a page: a thread a job, going through its bricks in turn.
[shader("compute")]
[numthreads(64, 1, 1)]
void voxelWorldPack(uint3 id: SV_DispatchThreadID)
{
    WorldMake* make = push.make;
    uint job = id.x;
    if (job >= make->jobCount)
    {
        return;
    }
    uint slot = make->jobs[job].slot;
    uint total = 0;
    uint named = 0;
    for (uint s = 0; s < make->tall; s++)
    {
        total += make->counters[SECTOR_COUNTS + job * 16 + s];
        named += make->counters[TREE_COUNTS + job * 16 + s];
    }
    // The column's own bricks, then the names of its trees' bricks, so many to a brick of room.
    total += (named + NAMES_A_BRICK - 1u) / NAMES_A_BRICK;
    uint wanted = min((total + PAGE_BRICKS - 1u) / PAGE_BRICKS, SLOT_PAGES);
    uint got = 0;
    while (got < wanted)
    {
        uint page = takePage(make, 0u);
        if (page == UNPLACED)
        {
            break;
        }
        make->pages[slot * SLOT_PAGES + got] = page;
        got++;
    }
    uint materialPages = 0;
    uint used = MATERIAL_PAGE;
    uint at = 0;
    uint bricks = min(make->counters[job * COUNTERS], make->brickRoom);
    for (uint k = 0; k < bricks; k++)
    {
        uint record = make->loose[(job * make->brickRoom + k) * BRICK_WORDS + 18];
        uint bytes = record == UNPLACED ? 0u : ((record >> 20) + 3u) & ~3u;
        uint where = UNPLACED;
        if (bytes > 0u)
        {
            if (used + bytes > MATERIAL_PAGE && materialPages < SLOT_MATERIAL_PAGES)
            {
                uint page = takePage(make, 1u);
                if (page != UNPLACED)
                {
                    make->materialPages[slot * SLOT_MATERIAL_PAGES + materialPages] = page;
                    materialPages++;
                    at = page * MATERIAL_PAGE;
                    used = 0;
                }
            }
            if (used + bytes <= MATERIAL_PAGE)
            {
                where = at + used;
                used += bytes;
            }
        }
        make->placed[job * make->brickRoom + k] = where;
    }
    make->slotCounts[slot] = got | (materialPages << 16);
}

groupshared uint sectorMask[16];
groupshared uint sectorTrees[16];
groupshared uint sectorFirst;
groupshared uint sectorNames;
groupshared uint sectorUnplaced;

// Puts each sector's bricks in its slot's pages, in the order of its bits after the sectors under it, with their
// materials, and writes its entry: where it is in the world, its bits, its slot and first brick in the slot's pages,
// which of its bricks are trees' and where the names of the plants' bricks they are start among the slot's words,
// after all its own bricks. The walk finds its bricks by them (voxels.slang). A sector any of whose bricks has no room
// is left empty.
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
        sectorTrees[index] = 0;
    }
    if (index == 0)
    {
        sectorUnplaced = 0;
    }
    GroupMemoryBarrierWithGroupSync();
    int3 l = int3(index & 7u, (index >> 3) & 7u, index >> 6);
    uint row = sy * 8 + uint(l.y);
    uint rows = make->tall * 8;
    uint kept = make->slots[job * 64 * rows + row * 64 + uint(l.z * 8 + l.x)];
    uint slot = make->jobs[job].slot;
    bool named = (kept & TREE_BRICK) != 0;
    if (kept != 0)
    {
        InterlockedOr(sectorMask[index >> 5], 1u << (index & 31u));
        if (named)
        {
            InterlockedOr(sectorTrees[index >> 5], 1u << (index & 31u));
        }
        else if (make->placed[job * make->brickRoom + kept - 1] == UNPLACED)
        {
            sectorUnplaced = 1;
        }
    }
    GroupMemoryBarrierWithGroupSync();
    if (index == 0)
    {
        uint count = 0;
        uint trees = 0;
        for (uint w = 0; w < 16; w++)
        {
            count += countbits(sectorMask[w]);
            trees += countbits(sectorTrees[w]);
        }
        uint first = 0;
        uint firstName = 0;
        uint own = 0;
        for (uint s = 0; s < make->tall; s++)
        {
            uint here = make->counters[SECTOR_COUNTS + job * 16 + s];
            own += here;
            if (s < sy)
            {
                first += here;
                firstName += make->counters[TREE_COUNTS + job * 16 + s];
            }
        }
        uint room = (make->slotCounts[slot] & 0xFFFFu) * PAGE_BRICKS;
        uint names = own * NAMES_A_BRICK + firstName;
        bool fits = first + count - trees <= room && (names + trees + NAMES_A_BRICK - 1u) / NAMES_A_BRICK <= room;
        sectorFirst = (count > 0 && sectorUnplaced == 0 && fits) ? (slot << 12) | first : EMPTY;
        sectorNames = names;
    }
    GroupMemoryBarrierWithGroupSync();
    uint w = make->window;
    uint* entry = make->table + ((sy * w * w) + slot) * ENTRY_WORDS;
    int2 column = make->jobs[job].column;
    bool empty = sectorFirst == EMPTY;
    if (index == 0)
    {
        entry[0] = uint(column.x);
        entry[1] = sy;
        entry[2] = uint(column.y);
    }
    else if (index >= 3 && index < 19)
    {
        entry[index] = empty ? 0u : sectorMask[index - 3];
    }
    else if (index >= 19 && index < 27)
    {
        // Counts of the sector's own bricks before each pair of words.
        uint pair = index - 19;
        uint before = 0;
        for (uint k = 0; k < pair * 2; k++)
        {
            before += countbits(sectorMask[k] & ~sectorTrees[k]);
        }
        entry[index] = before | ((before + countbits(sectorMask[pair * 2] & ~sectorTrees[pair * 2])) << 16);
    }
    else if (index == 27)
    {
        entry[27] = sectorFirst;
    }
    else if (index >= 28 && index < 44)
    {
        entry[index] = empty ? 0u : sectorTrees[index - 28];
    }
    else if (index >= 44 && index < 52)
    {
        uint pair = index - 44;
        uint before = 0;
        for (uint k = 0; k < pair * 2; k++)
        {
            before += countbits(sectorTrees[k]);
        }
        entry[index] = before | ((before + countbits(sectorTrees[pair * 2])) << 16);
    }
    else if (index == 52)
    {
        entry[52] = sectorNames;
    }
    if (kept == 0 || empty)
    {
        return;
    }
    uint mask = named ? sectorTrees[index >> 5] : sectorMask[index >> 5] & ~sectorTrees[index >> 5];
    uint rank = countbits(mask & ((1u << (index & 31u)) - 1u));
    for (uint k = 0; k < (index >> 5); k++)
    {
        rank += countbits(named ? sectorTrees[k] : sectorMask[k] & ~sectorTrees[k]);
    }
    if (named)
    {
        // The plant's brick's name, a word among the slot's words after its own bricks.
        uint at = sectorNames + rank;
        uint logical = at / NAMES_A_BRICK;
        uint brick = make->pages[slot * SLOT_PAGES + logical / PAGE_BRICKS] * PAGE_BRICKS + logical % PAGE_BRICKS;
        make->bricks[brick * BRICK_WORDS + at % NAMES_A_BRICK] = kept & ~TREE_BRICK;
        return;
    }
    uint logical = (sectorFirst & 0xFFFu) + rank;
    uint brick = make->pages[slot * SLOT_PAGES + logical / PAGE_BRICKS] * PAGE_BRICKS + logical % PAGE_BRICKS;
    uint* from = make->loose + (job * make->brickRoom + kept - 1) * BRICK_WORDS;
    uint* to = make->bricks + brick * BRICK_WORDS;
    uint where = make->placed[job * make->brickRoom + kept - 1];
    for (uint k = 0; k < BRICK_WORDS; k++)
    {
        to[k] = k == 18 ? where : from[k];
    }
    uint materials = from[18];
    uint words = (((materials >> 20) + 3u) & ~3u) / 4u;
    uint source = (job * make->materialRoom + (materials & 0xFFFFFu)) / 4u;
    for (uint k = 0; k < words; k++)
    {
        make->materials[where / 4u + k] = make->staging[source + k];
    }
}

// The generator run at places alone, for checking it against the CPU's: each of `candidateRoom` places, as the bits
// of its x, y and z in metres in the generator's world, read from `candidates`, and what is made there written to
// `loose`, sixteen words a place: the bits of its height, warmth, wetness and rock, how far into its Volume's ground
// a cell `voxel` metres across is there (with a Volume), then what its column carries up.
[shader("compute")]
[numthreads(64, 1, 1)]
void voxelWorldPlaces(uint3 id: SV_DispatchThreadID)
{
    WorldMake* make = push.make;
    if (id.x >= make->candidateRoom)
    {
        return;
    }
    float x = asfloat(make->candidates[id.x * 3]);
    float y = asfloat(make->candidates[id.x * 3 + 1]);
    float z = asfloat(make->candidates[id.x * 3 + 2]);
    float slots[GENERATOR_SLOTS];
    for (uint s = 0; s < GENERATOR_SLOTS; s++)
    {
        slots[s] = 0.0;
    }
    runSteps(make->program, make->programWords, x, 0.0, z, 0.0, make->program, slots);
    uint* out = make->loose + id.x * 16;
    uint4 outs = make->outs;
    out[0] = asuint(slots[min(outs.x, GENERATOR_SLOTS - 1)]);
    out[1] = asuint(slots[min(outs.y, GENERATOR_SLOTS - 1)]);
    out[2] = asuint(slots[min(outs.z, GENERATOR_SLOTS - 1)]);
    out[3] = asuint(outs.w < GENERATOR_SLOTS ? slots[outs.w] : 0.0);
    out[4] = 0;
    if (make->volumeWords == 0)
    {
        return;
    }
    for (uint c = 0; c < make->carriedCount; c++)
    {
        out[5 + c] = asuint(slots[min(make->carriedSlots[c >> 2][c & 3u], GENERATOR_SLOTS - 1)]);
    }
    float cells[GENERATOR_SLOTS];
    for (uint s = 0; s < GENERATOR_SLOTS; s++)
    {
        cells[s] = 0.0;
    }
    runSteps(make->volume, make->volumeWords, x, y, z, make->voxel, out + 5, cells);
    out[4] = asuint(cells[min(make->densitySlot, GENERATOR_SLOTS - 1)]);
}
