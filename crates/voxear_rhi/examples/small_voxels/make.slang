// The prototype's world made on the GPU, the same world `make` in main.rs makes on the CPU, in four passes:
//
// 1. makeColumns: each column's ground height (with a border of one round the world), and its grass blade and top.
// 2. makeCandidates: each column of bricks lists the bricks that ground, grass or a plant could reach.
// 3. makeBricks: one group a brick, a thread a voxel, works out each voxel, and keeps the brick if anything is in
//    it, its materials in the order of its bits, wherever there is room for now.
// 4. makeSectors: one group a sector puts its bricks one after another in the order of its bits, as the walk
//    wants them.
//
// Plants are placed on the CPU, which takes a few milliseconds, and listed by sector column.

struct MakeJob
{
    int* heights;
    uint* columns;
    int* plants;
    uint* plantLists;
    uint* plantIndices;
    uint* candidates;
    uint* counters;
    uint* slots;
    uint* loose;
    uint* bricks;
    uint* sectors;
    uint* materials;
    uint* palette;
    int3 sectorCount;
    float voxel;
    float2 camera;
    int across;
    uint candidateRoom;
};

struct MakePush
{
    MakeJob* job;
};

[[vk::push_constant]] ConstantBuffer<MakePush> push;

static const uint EMPTY = 0xFFFFFFFFu;
static const uint PLANT_INTS = 32;
static const uint CANDIDATES = 0;
static const uint BRICKS = 1;
static const uint MATERIAL_BYTES = 2;
static const uint PLACED = 3;
static const uint SOLID = 4;
static const uint DISPATCH = 8;
static const uint ROW = 1024;

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

float step01(float a, float b, float x)
{
    float t = clamp((x - a) / (b - a), 0.0, 1.0);
    return t * t * (3.0 - 2.0 * t);
}

float groundAt(MakeJob* job, float x, float z)
{
    float2 cam = job->camera;
    float base = 6.0 + 1.4 * fbm(x / 30.0, z / 30.0, 1) + 0.4 * fbm(x / 6.0, z / 6.0, 2);
    float side = step01(25.0, 95.0, abs(x - cam.x)) * (12.0 + 12.0 * fbm(x / 50.0, z / 50.0, 3));
    float back = step01(cam.y + 90.0, cam.y + 220.0, z) * 24.0 * (0.6 + 0.5 * fbm(x / 70.0, z / 70.0, 4));
    float behind = step01(cam.y - 10.0, cam.y - 60.0, z) * 10.0;
    return base + side + back + behind;
}

float forestAt(MakeJob* job, float x, float z)
{
    float2 cam = job->camera;
    float ahead = z - cam.y;
    float meadow = step01(55.0, 35.0, ahead) * step01(-15.0, 5.0, ahead) * step01(40.0, 25.0, abs(x - cam.x));
    return (0.25 + 0.75 * fbm(x / 40.0, z / 40.0, 9)) * (1.0 - meadow);
}

// Column records: the blade's height, its material and the top's material.
[shader("compute")]
[numthreads(8, 8, 1)]
void makeColumns(uint3 id: SV_DispatchThreadID)
{
    MakeJob* job = push.job;
    int side = job->across + 2;
    if (int(id.x) >= side || int(id.y) >= side)
    {
        return;
    }
    float v = job->voxel;
    int wx = int(id.x) - 1;
    int wz = int(id.y) - 1;
    int h = int(floor(groundAt(job, (float(wx) + 0.5) * v, (float(wz) + 0.5) * v) / v));
    job->heights[id.y * side + id.x] = h;
    if (wx < 0 || wz < 0 || wx >= job->across || wz >= job->across)
    {
        return;
    }
    float mx = float(wx) * v;
    float mz = float(wz) * v;
    float forest = forestAt(job, mx, mz);
    bool hill = float(h) * v > 13.0;
    uint r = hashOf(wx, 0, wz, 21);
    float chance = hill ? 0.35 : 0.62 - 0.3 * forest;
    uint blade = 0;
    uint bladeKind = 0;
    if (unit(r) < chance)
    {
        float tallest = (hill ? 0.25 : 0.7 - 0.37 * forest) / v;
        blade = 1 + uint(unit(hashOf(wx, 1, wz, 21)) * tallest);
        bladeKind = 3 + (r & 3u);
    }
    uint top = 1;
    if (hill)
    {
        top = fbm(mx / 5.0, mz / 5.0, 31) > 0.72 ? 21 : 19;
    }
    job->columns[wz * job->across + wx] = blade | (bladeKind << 8) | (top << 16);
}

int heightOf(MakeJob* job, int x, int z)
{
    return job->heights[(z + 1) * (job->across + 2) + x + 1];
}

int lowOf(MakeJob* job, int x, int z)
{
    int h = heightOf(job, x, z);
    int around = min(min(heightOf(job, x, z - 1), heightOf(job, x, z + 1)), min(heightOf(job, x - 1, z), heightOf(job, x + 1, z)));
    return min(around, h) - 1;
}

int* plantOf(MakeJob* job, uint index)
{
    return job->plants + index * PLANT_INTS;
}

// A plant's ints: kind (0 flower, 1 round tree, 2 cone), x, z, base, three of its own, then its bounds (low xyz,
// high xyz), two spare, and a round tree's four crowns (x, y, z, radius).
uint plantMaterial(int* plant, int3 p)
{
    int kind = plant[0];
    int x = plant[1];
    int z = plant[2];
    int base = plant[3];
    if (kind == 0)
    {
        int stem = plant[4];
        int flower = plant[5];
        int dx = p.x - x;
        int dy = p.y - base;
        int dz = p.z - z;
        int top = stem;
        if (dx == 0 && dz == 0 && dy > 0 && dy <= top)
        {
            return 12;
        }
        uint h = hashOf(p.x, p.y, p.z, 77);
        if (flower == 0)
        {
            int r2 = dx * dx + dz * dz;
            if ((dy == top + 1 || dy == top + 2) && r2 <= 8 && (r2 < 5 || unit(h) < 0.6))
            {
                return 8;
            }
        }
        else if (flower == 1)
        {
            if (dx >= 0 && dx <= 1 && dz >= 0 && dz <= 1 && dy > top - 5 && dy <= top + 2 && unit(h) < 0.85)
            {
                return 9;
            }
        }
        else if (flower == 2)
        {
            if (dx >= -1 && dx <= 1 && dz >= -1 && dz <= 1 && dy > top && dy <= top + 3)
            {
                return 10;
            }
        }
        else if (abs(dx) + abs(dz) <= 1 && dy == top + 1)
        {
            return 11;
        }
        return 0;
    }
    if (kind == 1)
    {
        int trunk = plant[4];
        if (abs(p.x - x) <= 1 && abs(p.z - z) <= 1 && p.y >= base && p.y <= base + trunk)
        {
            return 14;
        }
        for (uint k = 0; k < 4; k++)
        {
            int4 c = int4(plant[16 + k * 4], plant[17 + k * 4], plant[18 + k * 4], plant[19 + k * 4]);
            int3 d = p - c.xyz;
            int d2 = d.x * d.x + d.y * d.y + d.z * d.z;
            if (d2 <= c.w * c.w)
            {
                int shell = c.w - 9;
                if (d2 >= shell * shell)
                {
                    uint h = hashOf(p.x, p.y, p.z, 5);
                    if (unit(h) < 0.6)
                    {
                        return (h & 1u) == 0u ? 15 : 16;
                    }
                }
                return 0;
            }
        }
        return 0;
    }
    int height = plant[4];
    int radius = plant[5];
    int dx = p.x - x;
    int dy = p.y - base;
    int dz = p.z - z;
    if (abs(dx) <= 1 && abs(dz) <= 1 && dy >= 0 && dy < height * 9 / 10)
    {
        return 13;
    }
    float f = float(dy) / float(height);
    if (f < 0.18 || f > 1.0)
    {
        return 0;
    }
    float tiers = 7.0;
    float g = frac((f - 0.18) / 0.82 * tiers);
    float reach = float(radius) * (1.0 - (f - 0.18) / 0.82) * (1.0 - 0.45 * g) + 2.0;
    float d = sqrt(float(dx * dx + dz * dz));
    if (d <= reach && d >= reach - 9.0)
    {
        uint h = hashOf(p.x, p.y, p.z, 6);
        if (unit(h) < 0.62)
        {
            return (h & 1u) == 0u ? 17 : 18;
        }
    }
    return 0;
}

uint packPlace(int3 b)
{
    return uint(b.x) | (uint(b.y) << 11) | (uint(b.z) << 21);
}

int3 unpackPlace(uint packed)
{
    return int3(packed & 2047u, (packed >> 11) & 1023u, packed >> 21);
}

// Each column of bricks lists the bricks in it that anything could reach.
[shader("compute")]
[numthreads(64, 1, 1)]
void makeCandidates(uint3 id: SV_DispatchThreadID)
{
    MakeJob* job = push.job;
    int bricksAcross = job->across / 8;
    if (int(id.x) >= bricksAcross * bricksAcross)
    {
        return;
    }
    int bx = int(id.x) % bricksAcross;
    int bz = int(id.x) / bricksAcross;
    int rows = job->sectorCount.y * 8;
    uint marked[12];
    for (uint i = 0; i < 12; i++)
    {
        marked[i] = 0;
    }
    int lowest = 0x7FFFFFFF;
    int highest = -0x7FFFFFFF;
    for (int z = 0; z < 8; z++)
    {
        for (int x = 0; x < 8; x++)
        {
            int cx = bx * 8 + x;
            int cz = bz * 8 + z;
            lowest = min(lowest, lowOf(job, cx, cz));
            highest = max(highest, heightOf(job, cx, cz) + int(job->columns[cz * job->across + cx] & 255u));
        }
    }
    for (int row = max(lowest >> 3, 0); row <= min(highest >> 3, rows - 1); row++)
    {
        marked[row >> 5] |= 1u << (row & 31);
    }
    int sectorsAcross = job->sectorCount.x;
    uint list = uint((bz / 8) * sectorsAcross + bx / 8) * 2;
    uint start = job->plantLists[list];
    uint count = job->plantLists[list + 1];
    for (uint k = 0; k < count; k++)
    {
        int* plant = plantOf(job, job->plantIndices[start + k]);
        int3 low = int3(plant[8], plant[9], plant[10]);
        int3 high = int3(plant[11], plant[12], plant[13]);
        if (low.x > bx * 8 + 7 || high.x < bx * 8 || low.z > bz * 8 + 7 || high.z < bz * 8)
        {
            continue;
        }
        for (int row = max(low.y >> 3, 0); row <= min(high.y >> 3, rows - 1); row++)
        {
            marked[row >> 5] |= 1u << (row & 31);
        }
    }
    for (uint w = 0; w < 12; w++)
    {
        uint bits = marked[w];
        while (bits != 0u)
        {
            uint bit = firstbitlow(bits);
            bits &= bits - 1u;
            uint slot;
            InterlockedAdd(job->counters[CANDIDATES], 1u, slot);
            if (slot < job->candidateRoom)
            {
                job->candidates[slot] = packPlace(int3(bx, int(w * 32 + bit), bz));
            }
        }
    }
}

// The group counts the next pass is dispatched with, ROW groups to a row.
[shader("compute")]
[numthreads(1, 1, 1)]
void makeArguments(uint3 id: SV_DispatchThreadID)
{
    MakeJob* job = push.job;
    uint count = min(job->counters[CANDIDATES], job->candidateRoom);
    job->counters[DISPATCH] = ROW;
    job->counters[DISPATCH + 1] = (count + ROW - 1) / ROW;
    job->counters[DISPATCH + 2] = 1;
}

groupshared uint sharedMask[16];
groupshared uint sharedGroups[2];
groupshared uint sharedSum[4];
groupshared uint sharedSlot;
groupshared uint sharedOffset;
groupshared uint sharedCount;

[shader("compute")]
[numthreads(512, 1, 1)]
void makeBricks(uint3 group: SV_GroupID, uint index: SV_GroupIndex)
{
    MakeJob* job = push.job;
    uint candidate = group.y * ROW + group.x;
    if (candidate >= min(job->counters[CANDIDATES], job->candidateRoom))
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
    int3 b = unpackPlace(job->candidates[candidate]);
    int3 l = int3(index & 7u, (index >> 3) & 7u, index >> 6);
    int3 p = b * 8 + l;
    int h = heightOf(job, p.x, p.z);
    uint column = job->columns[p.z * job->across + p.x];
    int blade = int(column & 255u);
    uint m = 0;
    if (p.y <= h && p.y >= lowOf(job, p.x, p.z))
    {
        m = p.y == h ? (column >> 16) & 255u : 2u;
    }
    else if (p.y > h && p.y <= h + blade)
    {
        m = (p.y == h + blade && float(blade) * job->voxel > 0.2) ? 7u : (column >> 8) & 255u;
    }
    else
    {
        uint list = uint((b.z / 8) * job->sectorCount.x + b.x / 8) * 2;
        uint start = job->plantLists[list];
        uint count = job->plantLists[list + 1];
        for (uint k = 0; k < count && m == 0; k++)
        {
            int* plant = plantOf(job, job->plantIndices[start + k]);
            int3 low = int3(plant[8], plant[9], plant[10]);
            int3 high = int3(plant[11], plant[12], plant[13]);
            if (any(p < low) || any(p > high))
            {
                continue;
            }
            m = plantMaterial(plant, p);
        }
    }
    if (m != 0)
    {
        InterlockedOr(sharedMask[index >> 5], 1u << (index & 31u));
        uint g = uint((l.x >> 1) + 4 * ((l.y >> 1) + 4 * (l.z >> 1)));
        InterlockedOr(sharedGroups[g >> 5], 1u << (g & 31u));
        // The average a far brick is drawn in leaves out the dirt under the surface.
        if (m != 2)
        {
            uint colour = job->palette[m];
            InterlockedAdd(sharedSum[0], colour & 255u);
            InterlockedAdd(sharedSum[1], (colour >> 8) & 255u);
            InterlockedAdd(sharedSum[2], (colour >> 16) & 255u);
            InterlockedAdd(sharedSum[3], 1u);
        }
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
        if (count > 0)
        {
            InterlockedAdd(job->counters[BRICKS], 1u, sharedSlot);
            InterlockedAdd(job->counters[MATERIAL_BYTES], count, sharedOffset);
            InterlockedAdd(job->counters[SOLID], count);
        }
    }
    GroupMemoryBarrierWithGroupSync();
    if (sharedCount == 0)
    {
        return;
    }
    if (m != 0)
    {
        uint rank = countbits(sharedMask[index >> 5] & ((1u << (index & 31u)) - 1u));
        for (uint w = 0; w < (index >> 5); w++)
        {
            rank += countbits(sharedMask[w]);
        }
        uint at = sharedOffset + rank;
        InterlockedOr(job->materials[at >> 2], m << ((at & 3u) * 8u));
    }
    uint* record = job->loose + sharedSlot * 20;
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
        record[18] = sharedOffset;
    }
    else if (index == 19)
    {
        uint3 avg;
        if (sharedSum[3] > 0)
        {
            avg = uint3(sharedSum[0], sharedSum[1], sharedSum[2]) / sharedSum[3];
        }
        else
        {
            uint dirt = job->palette[2];
            avg = uint3(dirt & 255u, (dirt >> 8) & 255u, (dirt >> 16) & 255u);
        }
        record[19] = avg.x | (avg.y << 8) | (avg.z << 16);
    }
    else if (index == 20)
    {
        int bricksAcross = job->across / 8;
        job->slots[(b.y * bricksAcross + b.z) * bricksAcross + b.x] = sharedSlot + 1;
    }
}

groupshared uint sectorMask[16];
groupshared uint sectorFirst;

[shader("compute")]
[numthreads(512, 1, 1)]
void makeSectors(uint3 sector: SV_GroupID, uint index: SV_GroupIndex)
{
    MakeJob* job = push.job;
    if (index < 16)
    {
        sectorMask[index] = 0;
    }
    GroupMemoryBarrierWithGroupSync();
    int3 l = int3(index & 7u, (index >> 3) & 7u, index >> 6);
    int3 b = int3(sector) * 8 + l;
    int bricksAcross = job->across / 8;
    uint slot = job->slots[(b.y * bricksAcross + b.z) * bricksAcross + b.x];
    if (slot != 0)
    {
        InterlockedOr(sectorMask[index >> 5], 1u << (index & 31u));
    }
    GroupMemoryBarrierWithGroupSync();
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
            InterlockedAdd(job->counters[PLACED], count, sectorFirst);
        }
    }
    GroupMemoryBarrierWithGroupSync();
    uint* record = job->sectors + uint((int(sector.y) * job->sectorCount.z + int(sector.z)) * job->sectorCount.x + int(sector.x)) * 25;
    if (index < 16)
    {
        record[index] = sectorMask[index];
    }
    else if (index < 24)
    {
        uint pair = index - 16;
        uint before = 0;
        for (uint w = 0; w < pair * 2; w++)
        {
            before += countbits(sectorMask[w]);
        }
        record[index] = before | ((before + countbits(sectorMask[pair * 2])) << 16);
    }
    else if (index == 24)
    {
        record[24] = sectorFirst;
    }
    if (slot != 0)
    {
        uint rank = countbits(sectorMask[index >> 5] & ((1u << (index & 31u)) - 1u));
        for (uint w = 0; w < (index >> 5); w++)
        {
            rank += countbits(sectorMask[w]);
        }
        uint* from = job->loose + (slot - 1) * 20;
        uint* to = job->bricks + (sectorFirst + rank) * 20;
        for (uint k = 0; k < 20; k++)
        {
            to[k] = from[k];
        }
    }
}
