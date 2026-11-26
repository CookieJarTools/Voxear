// Voxel Volumes (src/voxel_volume.rs): voxels a few centimetres across, kept as sparse bricks and drawn by walking
// each pixel's ray through them, with no meshes. A volume keeps three levels (src/voxel_volume/bricks.rs):
//
// - a sector is 8x8x8 bricks, 64 voxels a side: 512 bits for its bricks, 16-bit counts of the bits before each pair
//   of words, and where its bricks start, which follow one another in the order of its bits;
// - a brick is 8x8x8 voxels: 512 bits for its voxels, 64 for its 2x2x2 groups, where its materials start, and its
//   average colour;
// - a material is a byte a solid voxel, in the order of its brick's bits: a place in the volume's palette.
//
// The walk steps over a whole empty sector, a whole empty brick, an empty group or an empty voxel at a time. Where
// a voxel is under half a pixel it stops at the groups, and under an eighth at whole bricks in their average
// colour; a ray that has taken many steps goes coarser the same way. A brick of leaves or grass that sways in the
// wind is walked whole with the ray moved back by its sway (voxelSwayedBrick).
//
// The passes, as blocks have them: a coarse pass first walks one ray every COARSE_STEP pixels against whole
// bricks, so every pixel's ray starts near what it meets (voxelCoarse); the depth pass writes the frame's depth and
// each pixel's normal and roughness (voxelDepth); the opaque pass lights each pixel as the engine lights any surface,
// with the sun's shadows of everything else, the voxels' own shadow traced through them, the lamps, the sky's light
// and the GI, then hazes it (voxelOpaque). Both draw a triangle over the screen and write their own depth.

#include "shading.slang"
#include "wind.slang"

static const uint VOXEL_SECTOR_WORDS = 25;
static const uint VOXEL_ENTRY_WORDS = 53;
static const uint VOXEL_BRICK_WORDS = 20;
static const uint VOXEL_EMPTY = 0xFFFFFFFFu;
static const int VOXEL_MOST_STEPS = 1024;
static const float VOXEL_STEP_COARSENING = 0.01;
// A brick of leaves, marked over its average colour (src/voxel_volume/bricks.rs): its groups are loosely filled as its
// voxels are, and a group of it takes the colour of a voxel of its own.
static const uint VOXEL_LEAFY = 1u << 24;
// What moves a brick in the wind, in two bits over that: nothing; its grass and flowers, leant from the brick's foot;
// or its leaves, the whole brick swaying.
static const uint VOXEL_SWAY_SHIFT = 25u;
static const uint VOXEL_SWAY_GRASS = 1u;
static const uint VOXEL_SWAY_LEAVES = 2u;
// How far leaves sway at strength 1, and how far the tip of a brick of grass leans for each voxel up, in voxels; and
// the most either moves.
static const float VOXEL_LEAF_SWAY = 1.2;
static const float VOXEL_GRASS_LEAN = 0.2;
static const float VOXEL_SWAY_MOST = 2.0;
// How far a voxel's shadow is traced towards the sun, in metres.
static const float VOXEL_SHADOW_REACH = 64.0;

struct VoxelHit
{
    // How far along the ray, in metres, and in which volume.
    float metres;
    uint volume;
    float t;
    int axis;
    int3 cell;
    int brick;
    uint level;
    int steps;
};

bool voxelBit(uint* words, uint index)
{
    return ((words[index >> 5] >> (index & 31u)) & 1u) != 0u;
}

uint* voxelSector(VoxelVolume volume, int3 s)
{
    if (volume.windowed == 0u)
    {
        return volume.sectors + uint((s.y * volume.sectorCount.z + s.z) * volume.sectorCount.x + s.x) * VOXEL_SECTOR_WORDS;
    }
    // An endless volume's sectors are kept by where they are in the world, wrapping round its window; each slot's
    // entry says which sector it holds, then holds it as a volume's sector is held.
    int3 world = volume.windowSector + s;
    int w = volume.sectorCount.x;
    // Wrapped as the CPU wraps it (rem_euclid), from whole numbers made positive first: a remainder of a negative
    // number is not taken the same way everywhere.
    uint2 slot = uint2(uint(world.x + w * 65536), uint(world.z + w * 65536)) % uint(w);
    uint* entry = volume.sectors + ((uint(s.y) * uint(w) + slot.y) * uint(w) + slot.x) * VOXEL_ENTRY_WORDS;
    if (int(entry[0]) != world.x || int(entry[1]) != world.y || int(entry[2]) != world.z)
    {
        return volume.emptySector;
    }
    return entry + 3;
}

// The brick at brick cell `b`: its number, -1 for none, -2 outside the volume.
int voxelBrickAt(VoxelVolume volume, int3 b)
{
    int3 s = b >> 3;
    if (any(s < 0) || any(s >= volume.sectorCount))
    {
        return -2;
    }
    uint* sector = voxelSector(volume, s);
    if (sector[24] == VOXEL_EMPTY)
    {
        return -1;
    }
    int3 l = b & 7;
    uint index = uint(l.x + 8 * (l.y + 8 * l.z));
    uint word = sector[index >> 5];
    uint bit = index & 31u;
    if (((word >> bit) & 1u) == 0u)
    {
        return -1;
    }
    uint below = (1u << bit) - 1u;
    uint pair = (index >> 5) & 1u;
    if (volume.windowed == 0u)
    {
        uint before = (sector[16 + (index >> 6)] >> (pair * 16u)) & 0xFFFFu;
        return int(sector[24] + before + countbits(word & below));
    }
    // An endless volume's sector names its slot and where its bricks start among the slot's, which are kept in pages
    // of the shared room (src/voxel_volume/endless.rs). A brick that is a tree's alone is its plant's, at the start
    // of the room: the slot names it, a word a brick, after its own bricks.
    uint slot = sector[24] >> 12;
    uint trees = sector[25 + (index >> 5)];
    uint at;
    if (((trees >> bit) & 1u) != 0u)
    {
        uint name = sector[49] + ((sector[41 + (index >> 6)] >> (pair * 16u)) & 0xFFFFu) + countbits(trees & below);
        at = name / VOXEL_BRICK_WORDS;
        uint brick = volume.pages[slot * 64u + (at >> 6)] * 64u + (at & 63u);
        return int(volume.bricks[brick * VOXEL_BRICK_WORDS + name % VOXEL_BRICK_WORDS]);
    }
    at = (sector[24] & 0xFFFu) + ((sector[16 + (index >> 6)] >> (pair * 16u)) & 0xFFFFu) + countbits(word & ~trees & below);
    return int(volume.pages[slot * 64u + (at >> 6)] * 64u + (at & 63u));
}

bool voxelSolid(VoxelVolume volume, int3 cell)
{
    int brick = voxelBrickAt(volume, cell >> 3);
    if (brick < 0)
    {
        return false;
    }
    int3 l = cell & 7;
    return voxelBit(volume.bricks + uint(brick) * VOXEL_BRICK_WORDS, uint(l.x + 8 * (l.y + 8 * l.z)));
}

// How the wind moves the brick at brick cell `b` of a volume at `time`, whose voxels sway as `kind` says: how far its
// leaves are moved, and how far its grass leans across for each voxel up from the brick's foot, in voxels. Downwind as
// hard as the wind and the gust over it blow, to and fro and across at a pace that drifts slowly over the ground, so a
// crown stirs while a gust still crosses it whole, as block leaves sway (blockSway), and neighbouring bricks move
// nearly alike. As far as the game's foliage bends (Wind.bend).
void voxelSwayOf(VoxelVolume volume, int3 b, uint kind, float time, out float3 moved, out float2 lean)
{
    moved = 0.0;
    lean = 0.0;
    Frame* frame = push.frame;
    Wind wind = frame->wind;
    if (kind == 0u || wind.strength <= 0.0 || wind.bend <= 0.0)
    {
        return;
    }
    float3 world = levelPosition(frame, volume.corner + (float3(b * 8) + 4.0) * volume.voxel, 4096.0);
    // The gust over it drifting downwind, as windPush's, in one noise; and its pace, drifting slowly over the ground.
    float2 drift = world.xz / wind.gustSize - wind.direction * (time * wind.gustSpeed / wind.gustSize);
    float blow = clamp(wind.strength * (1.0 + wind.gusts * (windNoise(drift) * 2.0 - 0.6)), 0.0, 2.0);
    float phase = windNoise(world.xz * 0.3 + world.y * 0.2) * 6.2831853;
    float stir = 0.4 + 0.6 * saturate(wind.turbulence);
    float to = sin(time * 2.1 + phase) * 0.6 + sin(time * 3.7 + phase * 1.7) * 0.4;
    float side = sin(time * 2.9 + phase * 2.3);
    float3 along = float3(wind.direction.x, 0.0, wind.direction.y);
    float3 across = float3(-wind.direction.y, 0.0, wind.direction.x);
    float3 swayed = (along * (0.5 + 0.5 * to * stir) + across * (side * 0.45 * stir)) * (blow * wind.bend);
    if (kind == VOXEL_SWAY_LEAVES)
    {
        swayed.y += sin(time * 3.3 + phase * 1.3) * 0.3 * stir * blow * wind.bend;
        moved = swayed * VOXEL_LEAF_SWAY;
        float far = length(moved);
        moved *= far > VOXEL_SWAY_MOST ? VOXEL_SWAY_MOST / far : 1.0;
    }
    else
    {
        lean = swayed.xz * VOXEL_GRASS_LEAN;
        float far = length(lean) * 8.0;
        lean *= far > VOXEL_SWAY_MOST ? VOXEL_SWAY_MOST / far : 1.0;
    }
}

// The first voxel (or 2x2x2 group, `level` 1) the ray from `o` along `d` meets from `t` on in the brick at brick cell
// `b`, whose voxels the wind has moved by `moved` and leant by `lean` (voxelSwayOf): walked in the brick's voxels as
// they stand, the ray moved back and its lean undone, which keeps it straight, so `t` is the ray's own. Voxels moved
// out of the brick are found as far as the ray goes in it; a voxel moved into it from a brick beside it is not.
bool voxelSwayedBrick(uint* bits, int3 b, float3 o, float3 d, float t, int axisIn, float3 moved, float2 lean, uint level, inout VoxelHit hit)
{
    float3 so = o - moved - float3(b * 8);
    so.xz -= lean * so.y;
    float3 sd = d;
    sd.xz -= lean * d.y;
    sd = select(abs(sd) < 1e-7, select(sd < 0.0, -1e-7, 1e-7), sd);
    float3 inv = 1.0 / sd;
    float3 a0 = -so * inv;
    float3 a1 = (8.0 - so) * inv;
    float3 tn = min(a0, a1);
    float3 tf = max(a0, a1);
    float enter = max(max(tn.x, tn.y), tn.z);
    float exit = min(min(tf.x, tf.y), tf.z);
    int axis = enter > t ? (tn.x >= tn.y && tn.x >= tn.z ? 0 : (tn.y >= tn.z ? 1 : 2)) : axisIn;
    float at = max(enter, t);
    int size = level == 1u ? 2 : 1;
    int3 up = int3(sd > 0.0);
    for (int k = 0; k < 26 && at < exit; k++)
    {
        int3 q = clamp(int3(floor(so + sd * min(at + 0.001, exit))), int3(0), int3(7));
        bool solid = level == 1u ? voxelBit(bits + 16, uint((q.x >> 1) + 4 * ((q.y >> 1) + 4 * (q.z >> 1))))
                                 : voxelBit(bits, uint(q.x + 8 * (q.y + 8 * q.z)));
        if (solid)
        {
            hit.t = at;
            hit.axis = axis;
            hit.cell = b * 8 + q;
            hit.level = level;
            return true;
        }
        float3 edge = float3((q & ~(size - 1)) + up * size);
        float3 te = (edge - so) * inv;
        axis = te.x < te.y && te.x < te.z ? 0 : (te.y < te.z ? 1 : 2);
        at = max(te[axis], at);
    }
    return false;
}

// Walks the ray from `o` along `d` (both in the volume's voxels, `o` from its corner) to `tMax` voxels. A voxel is
// `foot0 + t * footScale` voxels to a pixel at distance `t`, which picks the level it stops at.
bool voxelTrace(VoxelVolume volume, float3 o, float3 d, float tMax, float foot0, float footScale, out VoxelHit hit)
{
    hit = (VoxelHit)0;
    hit.axis = -1;
    d = select(abs(d) < 1e-7, select(d < 0.0, -1e-7, 1e-7), d);
    float3 inv = 1.0 / d;
    int3 dims = volume.sectorCount * 64;
    float3 t0 = -o * inv;
    float3 t1 = (float3(dims) - o) * inv;
    float3 tn = min(t0, t1);
    float3 tf = max(t0, t1);
    float tEnter = max(max(tn.x, tn.y), max(tn.z, 0.0));
    float tExit = min(min(tf.x, tf.y), min(tf.z, tMax));
    if (tEnter >= tExit)
    {
        return false;
    }
    int3 up = int3(d > 0.0);
    int axis = -1;
    int boundary = 0;
    if (tEnter > 0.0)
    {
        axis = tn.x >= tn.y && tn.x >= tn.z ? 0 : (tn.y >= tn.z ? 1 : 2);
        boundary = d[axis] > 0.0 ? 0 : dims[axis];
    }
    float t = tEnter;
    float time = push.frame->time;
    int lastBrick = -1;
    uint sway = 0;
    for (int i = 0; i < VOXEL_MOST_STEPS; i++)
    {
        if (t >= tExit)
        {
            hit.steps = i;
            return false;
        }
        // A ray going back along an axis is in the cell below a boundary it lies exactly on: else one starting on the
        // corner of two sectors goes back and forth between them without moving on.
        float3 p = o + d * t;
        int3 c = int3(select(d < 0.0, ceil(p) - 1.0, floor(p)));
        if (axis >= 0)
        {
            c[axis] = d[axis] > 0.0 ? boundary : boundary - 1;
        }
        c = clamp(c, int3(0), dims - 1);
        int size = 64;
        int brick = voxelBrickAt(volume, c >> 3);
        if (brick >= 0)
        {
            size = 8;
            uint* bits = volume.bricks + uint(brick) * VOXEL_BRICK_WORDS;
            float foot = foot0 + t * footScale + float(i) * VOXEL_STEP_COARSENING;
            int3 l = c & 7;
            hit.t = t;
            hit.axis = axis;
            hit.cell = c;
            hit.brick = brick;
            hit.steps = i;
            if (foot >= 8.0)
            {
                hit.level = 2;
                return true;
            }
            // A brick the wind moves is walked whole, as its voxels stand moved, then passed.
            if (brick != lastBrick)
            {
                lastBrick = brick;
                sway = (bits[19] >> VOXEL_SWAY_SHIFT) & 3u;
            }
            if (sway != 0u)
            {
                float3 moved;
                float2 lean;
                voxelSwayOf(volume, c >> 3, sway, time, moved, lean);
                if (voxelSwayedBrick(bits, c >> 3, o, d, t, axis, moved, lean, foot >= 2.0 ? 1u : 0u, hit))
                {
                    return true;
                }
                size = 8;
            }
            else if (foot >= 2.0)
            {
                int3 g = l >> 1;
                if (voxelBit(bits + 16, uint(g.x + 4 * (g.y + 4 * g.z))))
                {
                    hit.level = 1;
                    return true;
                }
                size = 2;
            }
            else
            {
                if (voxelBit(bits, uint(l.x + 8 * (l.y + 8 * l.z))))
                {
                    hit.level = 0;
                    return true;
                }
                size = 1;
            }
        }
        else if (brick == -1)
        {
            size = voxelSector(volume, c >> 6)[24] == VOXEL_EMPTY ? 64 : 8;
        }
        int3 low = c & ~(size - 1);
        float3 edge = float3(low + up * size);
        float3 te = (edge - o) * inv;
        axis = te.x < te.y && te.x < te.z ? 0 : (te.y < te.z ? 1 : 2);
        t = max(te[axis], t);
        boundary = int(edge[axis]);
    }
    hit.steps = VOXEL_MOST_STEPS;
    return false;
}

// The nearest voxel of any volume along the ray from `from` along `d` (in the world), starting `start` metres out
// and going no further than `reach`. `foot0` and `footScale` are as voxelTrace's, in metres.
bool voxelsTrace(VoxelVolumes* volumes, float3 from, float3 d, float start, float reach, float foot0, float footScale, out VoxelHit nearest)
{
    nearest = (VoxelHit)0;
    nearest.metres = reach;
    bool found = false;
    for (uint v = 0; v < volumes->count; v++)
    {
        VoxelVolume volume = volumes->volumes[v];
        float3 o = (from + d * start - volume.corner) / volume.voxel;
        VoxelHit hit;
        float far = (nearest.metres - start) / volume.voxel;
        if (far <= 0.0)
        {
            continue;
        }
        bool met = voxelTrace(volume, o, d, far, (foot0 + start * footScale) / volume.voxel, footScale, hit);
        nearest.steps += hit.steps;
        if (met)
        {
            hit.metres = start + hit.t * volume.voxel;
            hit.volume = v;
            hit.steps = nearest.steps;
            nearest = hit;
            found = true;
        }
    }
    return found;
}

float3 voxelColour(VoxelVolume volume, VoxelHit hit)
{
    uint* bits = volume.bricks + uint(hit.brick) * VOXEL_BRICK_WORDS;
    uint packed;
    bool leafGroup = hit.level == 1 && (bits[19] & VOXEL_LEAFY) != 0u;
    if (hit.level != 0 && !leafGroup)
    {
        packed = bits[19];
    }
    else
    {
        // A voxel's own colour; a group of leaves takes its first voxel's, so leaves keep their shades far off.
        int3 l = hit.cell & 7;
        if (leafGroup)
        {
            int3 g = l & ~1;
            for (uint k = 0; k < 8; k++)
            {
                l = g + int3(int(k & 1u), int((k >> 1) & 1u), int(k >> 2));
                if (voxelBit(bits, uint(l.x + 8 * (l.y + 8 * l.z))))
                {
                    break;
                }
            }
        }
        uint index = uint(l.x + 8 * (l.y + 8 * l.z));
        uint before = countbits(bits[index >> 5] & ((1u << (index & 31u)) - 1u));
        for (uint k = 0; k < (index >> 5); k++)
        {
            before += countbits(bits[k]);
        }
        uint at = bits[18] + before;
        uint material = (volume.materials[at >> 2] >> ((at & 3u) * 8u)) & 255u;
        packed = volume.palette[material];
    }
    float3 srgb = float3(packed & 255u, (packed >> 8) & 255u, (packed >> 16) & 255u) / 255.0;
    // Each voxel a shade of its own, so a field of one colour still reads as small cubes.
    uint3 c = uint3(hit.cell >> hit.level);
    uint h = c.x * 0x8DA6B343u ^ c.y * 0xD8163841u ^ c.z * 0xCB1AB31Fu;
    h ^= h >> 16;
    h *= 0x7FEB352Du;
    h ^= h >> 15;
    float grain = float(h >> 8) / 16777216.0;
    return pow(srgb, 2.2) * (0.9 + 0.2 * grain);
}

float3 voxelNormal(VoxelHit hit, float3 d)
{
    float3 n = float3(0.0);
    if (hit.axis >= 0)
    {
        n[hit.axis] = d[hit.axis] > 0.0 ? -1.0 : 1.0;
    }
    else
    {
        n = -d;
    }
    return n;
}

// The world-space ray through pixel `pixel` (in the drawn image's pixels), as the frame's jittered camera sees it.
float3 voxelRay(Frame* frame, float2 pixel)
{
    float2 ndc = pixel / float2(frame->viewport) * 2.0 - 1.0;
    float4 nearPoint = transform(frame->invViewProj, float4(ndc, 1.0, 1.0));
    float4 farther = transform(frame->invViewProj, float4(ndc, 0.5, 1.0));
    return normalize(farther.xyz / farther.w - nearPoint.xyz / nearPoint.w);
}

// Where the ray through `pixel` starts: the camera, or for an orthographic camera its point of the near plane.
float3 voxelOrigin(Frame* frame, float2 pixel)
{
    return eyeRay(frame, pixel / float2(frame->viewport) * 2.0 - 1.0).origin;
}

VoxelVolumes* voxelVolumes()
{
    return buffers()->voxelVolumes;
}

// How many metres across a pixel is at a metre out: the footprint the levels of detail go by.
float voxelPixelAngle(Frame* frame, float2 pixel)
{
    return length(voxelRay(frame, pixel + float2(0.0, 1.0)) - voxelRay(frame, pixel));
}

// The ray from the camera through `pixel` to the nearest voxel, starting where the coarse pass says it may.
bool voxelPixel(Frame* frame, float2 pixel, out float3 d, out VoxelHit hit)
{
    VoxelVolumes* volumes = voxelVolumes();
    d = voxelRay(frame, pixel);
    float footScale = voxelPixelAngle(frame, pixel) * volumes->lodScale;
    float start = 0.0;
    if (volumes->coarseStep != 0)
    {
        uint2 c = min(uint2(pixel) / volumes->coarseStep, volumes->coarseSize - 2);
        uint w = volumes->coarseSize.x;
        float* t = volumes->coarse;
        float nearest = min(min(t[c.y * w + c.x], t[c.y * w + c.x + 1]), min(t[(c.y + 1) * w + c.x], t[(c.y + 1) * w + c.x + 1]));
        if (nearest >= 1e9)
        {
            hit = (VoxelHit)0;
            return false;
        }
        // A brick and a half short, in the coarsest volume's bricks.
        start = max(nearest - 12.0 * volumes->volumes[0].voxel, 0.0);
    }
    return voxelsTrace(volumes, voxelOrigin(frame, pixel), d, start, 1e9, 0.0, footScale, hit);
}

// The coarse rays: each walks against whole bricks, as if each brick were solid, and keeps how far it went.
[shader("compute")]
[numthreads(8, 8, 1)]
void voxelCoarse(uint3 id: SV_DispatchThreadID)
{
    Frame* frame = push.frame;
    VoxelVolumes* volumes = voxelVolumes();
    if (id.x >= volumes->coarseSize.x || id.y >= volumes->coarseSize.y)
    {
        return;
    }
    float2 pixel = float2(id.xy * volumes->coarseStep);
    float3 d = voxelRay(frame, pixel);
    VoxelHit hit;
    // A footprint of eight voxels or more stops each walk at the first brick with anything in it.
    bool found = voxelsTrace(volumes, voxelOrigin(frame, pixel), d, 0.0, 1e9, 1e6, 0.0, hit);
    volumes->coarse[id.y * volumes->coarseSize.x + id.x] = found ? hit.metres : 1e10;
}

// How far out from a voxel its shadow ray starts, in its voxels, by the level it was met at: a coarser level's ray
// starts a group or a brick out, in the cell the view's ray came through, so it does not shadow itself.
float voxelLift(uint level)
{
    return level == 0 ? 0.02 : (level == 1 ? 2.02 : 8.02);
}

// Whether a voxel stands between `world`, `metres` from the camera through `pixel`, and the sun, its ray started
// `lift` metres out along `normal` and walked at the level the view's ray is.
bool voxelSunBlocked(Frame* frame, VoxelVolumes* volumes, float2 pixel, float3 world, float3 normal, float metres, float lift)
{
    float foot = metres * voxelPixelAngle(frame, pixel) * volumes->lodScale;
    VoxelHit blocked;
    return voxelsTrace(volumes, world + normal * lift, -frame->sunDirection, 0.0, VOXEL_SHADOW_REACH, foot, 0.0, blocked);
}

struct VoxelVertex
{
    float4 position : SV_Position;
};

[shader("vertex")]
VoxelVertex voxelVertex(uint vertexId: SV_VertexID)
{
    VoxelVertex o;
    o.position = float4(fullscreenNdc(vertexId), 0.0, 1.0);
    return o;
}

float voxelDepthOf(Frame* frame, float3 world)
{
    float4 clip = transform(frame->viewProj, float4(world, 1.0));
    return clip.z / clip.w;
}

struct VoxelDepthOut
{
    float4 surface : SV_Target0;
    float id : SV_Target1;
    float depth : SV_Depth;
};

[shader("fragment")]
VoxelDepthOut voxelDepth(VoxelVertex input)
{
    Frame* frame = push.frame;
    float3 d;
    VoxelHit hit;
    if (!voxelPixel(frame, input.position.xy, d, hit))
    {
        discard;
    }
    VoxelVolume volume = voxelVolumes()->volumes[hit.volume];
    float3 normal = voxelNormal(hit, d);
    float roughness = smoothedRoughness(frame, normal, volume.roughness);
    VoxelVolumes* volumes = voxelVolumes();
    if (volumes->seenOn != 0u)
    {
        uint2 at = uint2(input.position.xy);
        uint packed = uint(hit.cell.z) | (hit.level << 16) | (uint(hit.axis + 1) << 18) | (hit.volume << 20);
        volumes->seen[at.y * volumes->seenWidth + at.x] =
            uint4(uint(hit.cell.x) | (uint(hit.cell.y) << 16), packed, asuint(hit.metres), frame->frameIndex + 1);
    }
    VoxelDepthOut o;
    o.surface = float4(octEncode(normal) * 0.5 + 0.5, saturate(roughness), 1.0);
    o.id = asfloat(packSurfaceId(PACK_SURFACE_MODEL, 0));
    o.depth = voxelDepthOf(frame, voxelOrigin(frame, input.position.xy) + d * hit.metres);
    return o;
}

struct VoxelOut
{
    float4 color : SV_Target0;
    float2 velocity : SV_Target1;
    float depth : SV_Depth;
};

[shader("fragment")]
VoxelOut voxelOpaque(VoxelVertex input)
{
    Frame* frame = push.frame;
    float2 pixel = input.position.xy;
    float3 d;
    VoxelHit hit;
    VoxelVolumes* volumes = voxelVolumes();
    // What the depth pass met here this frame, where it kept it; else the ray walked again.
    uint4 seen = uint4(0);
    if (volumes->seenOn != 0u)
    {
        uint2 at = uint2(pixel);
        seen = volumes->seen[at.y * volumes->seenWidth + at.x];
    }
    if (seen.w == frame->frameIndex + 1)
    {
        d = voxelRay(frame, pixel);
        hit = (VoxelHit)0;
        hit.cell = int3(seen.x & 0xFFFFu, seen.x >> 16, seen.y & 0xFFFFu);
        hit.level = (seen.y >> 16) & 3u;
        hit.axis = int((seen.y >> 18) & 3u) - 1;
        hit.volume = seen.y >> 20;
        hit.metres = asfloat(seen.z);
        hit.brick = voxelBrickAt(volumes->volumes[hit.volume], hit.cell >> 3);
    }
    else if (!voxelPixel(frame, pixel, d, hit))
    {
        discard;
    }
    VoxelVolume volume = volumes->volumes[hit.volume];
    float3 world = voxelOrigin(frame, pixel) + d * hit.metres;
    float3 normal = voxelNormal(hit, d);
    float3 albedo = voxelColour(volume, hit);

    Surface s;
    s.normal = normal;
    s.view = -d;
    s.roughness = clamp(smoothedRoughness(frame, normal, volume.roughness), 0.045, 1.0);
    s.diffuse = albedo;
    s.f0 = float3(0.04);

    bool facingSun = dot(normal, -frame->sunDirection) > 0.0;
    float shadow = facingSun ? min(sunShadow(frame, world, normal), contactShadow(frame, pixel)) : 0.0;
    // The voxels are in no shadow map: their shadow on themselves, and on each other, is traced through them, by the
    // voxels' shadows pass where it is drawn (read with the contact shadows), else here.
    if (shadow > 0.0 && volumes->shadowMask == NO_TEXTURE
        && voxelSunBlocked(frame, volumes, pixel, world, normal, hit.metres, voxelLift(hit.level) * volume.voxel))
    {
        shadow = 0.0;
    }
    float2 blockLit = float2(1.0, 0.0);
    float4 blockGlow = 0.0;
    bool blockLitHere = blockLightAround(world + normal * 0.3, blockLit, blockGlow);
    if (blockLitHere)
    {
        shadow *= blockSunShare(blockLit.x, sunShadowsReach(frame, world));
    }
    bool atmosphere = frame->lighting.sky == SKY_ATMOSPHERE;
    float3 sunLight = atmosphere ? sunThroughAtmosphere(frame) : frame->sunColor;
    float3 sunThrough = sunLight * sunCookieAt(frame, world);
    sunThrough *= glassSun(frame, world, normal);
    float3 skyUnder;
    WaterOver water = waterOverAt(frame, world);
    sunThrough *= underwaterSunUnder(frame, world, water, skyUnder);
    float3 color = shade(s, -frame->sunDirection, sunThrough) * shadow;
    color += shadowTint(frame, s, -frame->sunDirection, sunThrough, shadow);

    float viewDepth = dot(world - frame->cameraPosition, frame->cameraForward);
    uint cluster = clusterAt(frame, pixel, viewDepth);
    uint clusterBase = cluster * (frame->clusterMaxLights + 1);
    uint clusterLights = buffers()->clusters[clusterBase];
    for (uint i = 0; i < clusterLights; i++)
    {
        uint light = buffers()->clusters[clusterBase + 1 + i];
        float3 wet = lampThroughWater(frame, buffers()->lights[light].position, world, water);
        color += shadeLocalLight(s, buffers()->lights[light], world) * wet;
    }

    float3 ambient = ambientLight(frame, world, s, sunLight, atmosphere);
    if (blockLitHere)
    {
        ambient = ambient * blockSkyShare(blockLit.x, blockGlow.w) + blockGlow.rgb * blockGlowShare(blockLit.y);
    }
    ambient *= skyUnder;
    float nv = saturate(dot(s.normal, s.view));
    float3 ambientFresnel = s.f0 + (max(float3(1.0 - s.roughness), s.f0) - s.f0) * pow(1.0 - nv, 5.0);
    Occlusion occlusion = ambientOcclusion(frame, pixel, nv, s.roughness);
    float found;
    float3 reflected = environmentReflection(frame, pixel, world, s.normal, reflect(-s.view, s.normal), s.roughness, ambient, atmosphere, found);
    float specularOcclusion = lerp(occlusion.specular, 1.0, found);
    color += ambient * s.diffuse * occlusion.diffuse;
    color += reflected * ambientFresnel * (1.0 - s.roughness * 0.7) * specularOcclusion;

    if (atmosphere)
    {
        float4 haze = aerialPerspectiveAt(frame, pixel / float2(frame->viewport), hit.metres);
        color = color * haze.a + haze.rgb * skyLook(frame);
    }
    color = cloudsBefore(frame, pixel / float2(frame->viewport), world, color);
    if (frame->debugView == DEBUG_VIEW_NORMALS)
    {
        color = s.normal * 0.5 + 0.5;
    }
    else if (frame->debugView == DEBUG_VIEW_UNLIT)
    {
        color = albedo;
    }
    else if (frame->debugView == DEBUG_VIEW_AMBIENT_OCCLUSION)
    {
        color = occlusion.diffuse;
    }
    else if (frame->debugView == DEBUG_VIEW_LOD)
    {
        color = debugColor(hit.level) * (0.35 + 0.65 * shadow * saturate(dot(s.normal, -frame->sunDirection)));
    }

    // A voxel the wind moves goes under a tenth of a voxel a frame, too little for the anti-aliasing to smear: its
    // motion is the camera's alone, which keeps the pass light.
    VoxelOut o;
    o.color = float4(color, 1.0);
    o.velocity = motion(transform(frame->unjitteredViewProj, float4(world, 1.0)), transform(frame->prevViewProj, float4(world, 1.0)));
    o.depth = voxelDepthOf(frame, world);
    return o;
}

// ---------------------------------------------------------------- The voxels' shadows on everything

// Whether the sun reaches each pixel past the voxels, for whatever the depth passes drew there: voxels, models,
// blocks, terrain and grass alike, 1 lit and 0 in a voxel's shadow, which their lighting reads with the contact
// shadows (contactShadow). The voxels are in no shadow map, so their shadow is traced from each pixel instead, through
// them towards the sun: from a voxel as its own lighting did, from anything else a voxel and a half out from its
// surface, or more where a pixel is wider. push.textures holds the depth and the depth passes' surfaces.
[shader("fragment")]
float4 voxelShadowFragment(VoxelVertex input) : SV_Target
{
    Frame* frame = push.frame;
    float2 pixel = input.position.xy;
    int2 at = int2(pixel);
    float depth = textures[NonUniformResourceIndex(push.textures.x)].Load(int3(at, 0)).r;
    if (depth <= 0.0 || dot(frame->sunColor, frame->sunColor) <= 0.0)
    {
        return 1.0;
    }
    VoxelVolumes* volumes = voxelVolumes();
    float3 eye = voxelOrigin(frame, pixel);
    float3 d = voxelRay(frame, pixel);
    float2 ndc = pixel / float2(frame->viewport) * 2.0 - 1.0;
    float4 placed = transform(frame->invViewProj, float4(ndc, depth, 1.0));
    float3 world = placed.xyz / placed.w;
    float metres = length(world - eye);
    float2 folded = textures[NonUniformResourceIndex(push.textures.y)].Load(int3(at, 0)).rg;
    float3 normal = octDecode(folded * 2.0 - 1.0);
    // A voxel the depth pass met here, where nothing drawn after it is nearer: from where its ray met it.
    float finest = volumes->volumes[volumes->count - 1].voxel;
    float lift = max(1.5 * finest, 2.0 * metres * voxelPixelAngle(frame, pixel));
    if (volumes->seenOn != 0u)
    {
        uint4 seen = volumes->seen[uint(at.y) * volumes->seenWidth + uint(at.x)];
        float met = asfloat(seen.z);
        uint volume = seen.y >> 20;
        float voxel = volumes->volumes[min(volume, volumes->count - 1)].voxel;
        if (seen.w == frame->frameIndex + 1 && abs(met - metres) < 0.01 * metres + 2.0 * voxel)
        {
            int axis = int((seen.y >> 18) & 3u) - 1;
            normal = 0.0;
            if (axis >= 0)
            {
                normal[axis] = d[axis] > 0.0 ? -1.0 : 1.0;
            }
            world = eye + d * met;
            metres = met;
            lift = voxelLift((seen.y >> 16) & 3u) * voxel;
        }
    }
    if (dot(normal, -frame->sunDirection) <= 0.0)
    {
        return 1.0;
    }
    return voxelSunBlocked(frame, volumes, pixel, world, normal, metres, lift) ? 0.0 : 1.0;
}

// ---------------------------------------------------------------- The sea floor

// The sea floor round the camera as water.slang keeps it (floorDepth): slices FLOOR_TEXELS a side in one depth image,
// two to a row, their tops in its left half and their undersides in its right, each texel as deep as its ground is
// high, from FLOOR_REACH under the sea's level to FLOOR_CEILING over it.
static const float VOXEL_FLOOR_REACH = 256.0;
static const float VOXEL_FLOOR_CEILING = 4.0;
static const float VOXEL_FLOOR_TEXELS = 256.0;

// The voxels under each texel of slice push.level of the sea floor, seen from straight above, as the meshes there are
// drawn (water.slang's floorVertex): with push.list 0, how high the highest voxel is, walked down from the floor's
// ceiling, as high as the ceiling where the walk starts in the ground; with push.list 1, how high the lowest is,
// walked up from the floor's reach. Nothing where the walk meets none.
[shader("fragment")]
float voxelFloorFragment(VoxelVertex input) : SV_Depth
{
    Frame* frame = push.frame;
    uint slice = push.level;
    bool under = push.list != 0;
    float2 tile = float2(float(slice % 2 + (under ? 2 : 0)), float(slice / 2)) * VOXEL_FLOOR_TEXELS;
    float4 placed = frame->ocean.floor[slice];
    float2 uv = (input.position.xy - tile) / VOXEL_FLOOR_TEXELS;
    float2 xz = placed.xy + uv * placed.z;
    float level = frame->ocean.level;
    float top = level + VOXEL_FLOOR_CEILING;
    float bottom = level - VOXEL_FLOOR_REACH;
    // Read to the voxels' groups at most, finer than a texel.
    float foot = placed.z / VOXEL_FLOOR_TEXELS * 0.25;
    // A hair off straight down, as the walk takes no ray wholly along an axis.
    float3 d = normalize(float3(1e-5, under ? 1.0 : -1.0, 1e-5));
    float3 from = float3(xz.x, under ? bottom : top, xz.y);
    VoxelHit hit;
    if (!voxelsTrace(voxelVolumes(), from, d, 0.0, top - bottom, foot, 0.0, hit))
    {
        discard;
    }
    float y = under ? bottom + hit.metres : top - hit.metres;
    return saturate((y - level + VOXEL_FLOOR_REACH) / (VOXEL_FLOOR_REACH + VOXEL_FLOOR_CEILING));
}
