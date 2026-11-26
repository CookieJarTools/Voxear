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
// How brightly a voxel glowing at its brightest shines, times its colour.
static const float VOXEL_GLOW_SHINE = 4.0;

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
    // The first see-through voxel the ray passed through on its way, as glass is: how far along it, as `t` is (-1 for
    // none) and in metres, its side, its volume and its colour's palette word.
    float glassT;
    float glassMetres;
    int glassAxis;
    uint glassVolume;
    uint glassColour;
};

// A brick with a voxel that glows, and one with a voxel that is see-through, marked in the lowest bits of its average
// red and green (src/voxel_volume/bricks.rs, voxel_world.slang).
static const uint VOXEL_GLOWS = 1u << 0;
static const uint VOXEL_SEE_THROUGH = 1u << 8;
// And one with water of voxels, in its average blue's (voxel_world.slang).
static const uint VOXEL_WET = 1u << 16;
// Where a palette's kinds start after its colours, a byte a place (PALETTE_KINDS in src/voxel_volume.rs).
static const uint VOXEL_PALETTE_KINDS = 4224u;

// How a voxel's kind, in the top byte of a colour as the glass pass keeps it, says it is see-through, from 0 for
// solid to 15, and how bright it glows, 0 to 15 (voxel_volume::kind_byte).
uint2 voxelKind(uint packed)
{
    return uint2((packed >> 24) & 15u, packed >> 28);
}

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
    // An endless volume's sectors are kept by where they are in the world, wrapping round its window and, up its
    // columns, round the band of layers each keeps; each slot's entry says which sector it holds, then holds it as a
    // volume's sector is held.
    int3 world = volume.windowSector + s;
    int w = volume.sectorCount.x;
    // Wrapped as the CPU wraps it (rem_euclid), from whole numbers made positive first: a remainder of a negative
    // number is not taken the same way everywhere.
    uint2 slot = uint2(uint(world.x + w * 65536), uint(world.z + w * 65536)) % uint(w);
    uint layer = uint(world.y) % volume.layers;
    uint* entry = volume.sectors + ((layer * uint(w) + slot.y) * uint(w) + slot.x) * VOXEL_ENTRY_WORDS;
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

// The palette place of a brick's voxel's material: below 128 a place every brick shares, above it a place in the bank
// the brick keeps over its average colour (src/voxel_volume/endless.rs), the first bank the palette's places 128 to 255.
uint voxelPlace(uint* bits, uint material)
{
    return material < 128u ? material : material + (bits[19] >> 27) * 128u;
}

// The palette place of the voxel at `l` in a brick, `bits` its words.
uint voxelPlaceAt(VoxelVolume volume, uint* bits, int3 l)
{
    uint index = uint(l.x + 8 * (l.y + 8 * l.z));
    uint before = countbits(bits[index >> 5] & ((1u << (index & 31u)) - 1u));
    for (uint k = 0; k < (index >> 5); k++)
    {
        before += countbits(bits[k]);
    }
    uint at = bits[18] + before;
    return voxelPlace(bits, (volume.materials[at >> 2] >> ((at & 3u) * 8u)) & 255u);
}

// The colour of the voxel at `l` in a brick, `bits` its words, as sRGB in its low three bytes.
uint voxelPaletteAt(VoxelVolume volume, uint* bits, int3 l)
{
    return volume.palette[voxelPlaceAt(volume, bits, l)];
}

// The kind of a palette place's colour: how see-through in its low four bits, how bright it glows in its high four.
uint voxelPlaceKind(VoxelVolume volume, uint place)
{
    return (volume.palette[VOXEL_PALETTE_KINDS + (place >> 2)] >> ((place & 3u) * 8u)) & 255u;
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
    hit.glassT = -1.0;
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
                    // A see-through voxel is passed through, the first kept; else the ray stops.
                    uint place = (bits[19] & VOXEL_SEE_THROUGH) != 0u ? voxelPlaceAt(volume, bits, l) : 0u;
                    uint kind = (bits[19] & VOXEL_SEE_THROUGH) != 0u ? voxelPlaceKind(volume, place) : 0u;
                    if ((kind & 15u) == 0u)
                    {
                        hit.level = 0;
                        return true;
                    }
                    if (hit.glassT < 0.0)
                    {
                        hit.glassT = t;
                        hit.glassAxis = axis;
                        hit.glassColour = (volume.palette[place] & 0xFFFFFFu) | (kind << 24);
                    }
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

// How far a ring's walk from `o` along `d` (in its voxels) passes over before it starts, and how far it goes at most,
// `far`, cut short: a ring draws only inside its outer box, a square round the camera, and passes over its hole, the
// ring inside it's outer box. Where it starts on an edge, which axis the edge is across (0 for X, 2 for Z) and where it
// is. A ray from the camera leaves the hole once and never comes back; a shadow ray going back over it meets the ring's
// own ground there, which is the finer ring's. `far` is none where the walk has nothing to go through.
float voxelRingSkip(float4 outer, float4 hole, float3 o, float3 d, inout float far, out int axis, out float edge)
{
    axis = 0;
    edge = 0.0;
    if (outer.z <= outer.x && hole.z <= hole.x)
    {
        return 0.0;
    }
    float2 inv = 1.0 / select(abs(d.xz) < 1e-7, select(d.xz < 0.0, -1e-7, 1e-7), d.xz);
    float into = 0.0;
    if (outer.z > outer.x)
    {
        float2 b0 = (outer.xy - o.xz) * inv;
        float2 b1 = (outer.zw - o.xz) * inv;
        float2 bn = min(b0, b1);
        float2 bf = max(b0, b1);
        far = min(far, min(bf.x, bf.y));
        into = max(max(bn.x, bn.y), 0.0);
        int across = bn.x > bn.y ? 0 : 1;
        axis = across * 2;
        edge = inv[across] > 0.0 ? outer[across] : outer[across + 2];
    }
    if (hole.z > hole.x)
    {
        float2 h0 = (hole.xy - o.xz) * inv;
        float2 h1 = (hole.zw - o.xz) * inv;
        float2 hn = min(h0, h1);
        float2 hf = max(h0, h1);
        float leave = min(hf.x, hf.y);
        if (max(hn.x, hn.y) <= into && leave > into)
        {
            into = leave;
            int across = hf.x < hf.y ? 0 : 1;
            axis = across * 2;
            edge = inv[across] > 0.0 ? hole[across + 2] : hole[across];
        }
    }
    if (into >= far)
    {
        far = 0.0;
    }
    return into;
}

// The nearest voxel of any volume along the ray from `from` along `d` (in the world), starting `start` metres out
// and going no further than `reach`. `foot0` and `footScale` are as voxelTrace's, in metres.
bool voxelsTrace(VoxelVolumes* volumes, float3 from, float3 d, float start, float reach, float foot0, float footScale, out VoxelHit nearest)
{
    nearest = (VoxelHit)0;
    nearest.metres = reach;
    bool found = false;
    // The nearest see-through voxel passed through, of any volume.
    VoxelHit glass = (VoxelHit)0;
    glass.glassT = -1.0;
    glass.glassMetres = reach;
    for (uint v = 0; v < volumes->count; v++)
    {
        VoxelVolume volume = volumes->volumes[v];
        // The corner taken from the camera first: far from the world's middle a place along the ray is a few millimetres
        // out, each pixel's its own way.
        float3 o = (from - volume.corner + d * start) / volume.voxel;
        VoxelHit hit;
        float far = (nearest.metres - start) / volume.voxel;
        // A ring's walk starts on the edge it passes over, so its first cell is the one past it. Worked out here, and
        // again after a hit, so the walk carries nothing more than a volume's.
        int axis;
        float edge;
        float skip = voxelRingSkip(volume.outer, volume.hole, o, d, far, axis, edge);
        if (far <= 0.0)
        {
            continue;
        }
        if (skip > 0.0)
        {
            o += d * skip;
            o[axis] = edge;
        }
        bool met = voxelTrace(volume, o, d, far - skip, (foot0 + (start + skip * volume.voxel) * footScale) / volume.voxel, footScale, hit);
        nearest.steps += hit.steps;
        if (hit.glassT >= 0.0 && start + (skip + hit.glassT) * volume.voxel < glass.glassMetres)
        {
            glass.glassT = skip + hit.glassT;
            glass.glassMetres = start + (skip + hit.glassT) * volume.voxel;
            glass.glassAxis = hit.glassAxis;
            glass.glassVolume = v;
            glass.glassColour = hit.glassColour;
        }
        if (met)
        {
            float4 outer = volumes->volumes[v].outer;
            float4 hole = volumes->volumes[v].hole;
            float voxel = volumes->volumes[v].voxel;
            float3 again = (from + d * start - volumes->volumes[v].corner) / voxel;
            float most = 1e30;
            skip = voxelRingSkip(outer, hole, again, d, most, axis, edge);
            // A voxel met first past an edge was cut by it: it is lit as its top, never as a side the edge made.
            if (skip > 0.0 && hit.t <= 0.0 && d.y < 0.0)
            {
                hit.axis = 1;
            }
            hit.t += skip;
            hit.metres = start + hit.t * voxel;
            hit.volume = v;
            hit.steps = nearest.steps;
            nearest = hit;
            found = true;
        }
    }
    bool before = glass.glassT >= 0.0 && glass.glassMetres < nearest.metres;
    nearest.glassT = before ? glass.glassT : -1.0;
    nearest.glassMetres = glass.glassMetres;
    nearest.glassAxis = glass.glassAxis;
    nearest.glassVolume = glass.glassVolume;
    nearest.glassColour = glass.glassColour;
    return found;
}

// The nearest voxel of any volume along the ray, as voxelsTrace, but walking every volume whole: rings' holes and outer
// boxes are not passed over. For the lit pass, which carries much else and runs best with the plainest walk: its
// fallback walk where no depth pass kept what a pixel met, and its shadow rays. A shadow ray from a voxel `nearVoxel`
// metres across (none for others) walks an endless volume of another size from three of the larger of their voxels
// out, and a ring whose hole it starts in from where it comes out of it: a ring's ground lies under the finer ring's
// near its edges, up to a voxel of its own over or under it, which the ray has climbed clear of by then; nearer than
// that, the ring met is the one walked.
bool voxelsPlainTrace(VoxelVolumes* volumes, float3 from, float3 d, float start, float reach, float foot0, float footScale, float nearVoxel, out VoxelHit nearest)
{
    nearest = (VoxelHit)0;
    nearest.metres = reach;
    bool found = false;
    for (uint v = 0; v < volumes->count; v++)
    {
        VoxelVolume volume = volumes->volumes[v];
        float begin = start;
        if (volume.windowed != 0u && nearVoxel > 0.0 && volume.voxel != nearVoxel)
        {
            begin += 3.0 * max(volume.voxel, nearVoxel);
            // A shadow ray starting in a ring's hole is walked in it from where it comes out: what is in the hole is
            // drawn by the finer ring, which the ray is walked through too.
            float2 at = (from.xz - volume.corner.xz) / volume.voxel;
            if (all(at > volume.hole.xy) && all(at < volume.hole.zw))
            {
                float2 leave = (select(d.xz > 0.0, volume.hole.zw, volume.hole.xy) - at) / d.xz;
                begin = max(begin, min(leave.x, leave.y) * volume.voxel);
            }
        }
        float3 o = (from + d * begin - volume.corner) / volume.voxel;
        VoxelHit hit;
        float far = (nearest.metres - begin) / volume.voxel;
        if (far <= 0.0)
        {
            continue;
        }
        bool met = voxelTrace(volume, o, d, far, (foot0 + begin * footScale) / volume.voxel, footScale, hit);
        nearest.steps += hit.steps;
        if (met)
        {
            hit.metres = begin + hit.t * volume.voxel;
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
        packed = voxelPaletteAt(volume, bits, l);
    }
    float3 srgb = float3(packed & 255u, (packed >> 8) & 255u, (packed >> 16) & 255u) / 255.0;
    // Each voxel a shade of its own, so a field of one colour still reads as small cubes: by its place in the world, so
    // an endless volume's window moving leaves it be, and a ring's voxel takes the shade of the finer ring's group of
    // its size where it stands.
    uint3 c = uint3((hit.cell + volume.windowSector * 64) >> hit.level);
    uint h = c.x * 0x8DA6B343u ^ c.y * 0xD8163841u ^ c.z * 0xCB1AB31Fu;
    h ^= h >> 16;
    h *= 0x7FEB352Du;
    h ^= h >> 15;
    float grain = float(h >> 8) / 16777216.0;
    return pow(srgb, 2.2) * (0.9 + 0.2 * grain);
}

// ---------------------------------------------------------------- Water of voxels

// The place in its volume's palette of the voxel at `cell` of brick `brick`, its material read through its brick's
// bank (voxelPlace).
uint voxelMaterialOf(VoxelVolume volume, int brick, int3 cell)
{
    uint* bits = volume.bricks + uint(brick) * VOXEL_BRICK_WORDS;
    int3 l = cell & 7;
    uint index = uint(l.x + 8 * (l.y + 8 * l.z));
    uint before = countbits(bits[index >> 5] & ((1u << (index & 31u)) - 1u));
    for (uint k = 0; k < (index >> 5); k++)
    {
        before += countbits(bits[k]);
    }
    uint at = bits[18] + before;
    return voxelPlace(bits, (volume.materials[at >> 2] >> ((at & 3u) * 8u)) & 255u);
}

// Whether material `material` of a volume is water of voxels, one of its shades.
bool voxelIsWater(VoxelVolume volume, uint material)
{
    return volume.water != 0u && material + 1u >= volume.water && material + 1u < volume.water + volume.shades;
}

// Whether a hit met a voxel of water. Far off, where the walk met a group or a brick, it is drawn in its brick's
// average colour as ground is: the water's own there.
bool voxelWaterOf(VoxelVolume volume, VoxelHit hit)
{
    if (volume.water == 0u || hit.brick < 0 || hit.level != 0)
    {
        return false;
    }
    return voxelIsWater(volume, voxelMaterialOf(volume, hit.brick, hit.cell));
}

// What the voxel at `cell` of a volume is: 0 open, 1 water, 2 anything else solid.
uint voxelKindAt(VoxelVolume volume, int3 cell)
{
    int brick = voxelBrickAt(volume, cell >> 3);
    if (brick < 0)
    {
        return 0u;
    }
    int3 l = cell & 7;
    if (!voxelBit(volume.bricks + uint(brick) * VOXEL_BRICK_WORDS, uint(l.x + 8 * (l.y + 8 * l.z))))
    {
        return 0u;
    }
    return voxelIsWater(volume, voxelMaterialOf(volume, brick, cell)) ? 1u : 2u;
}

// How far along the ray from `from` along `d` it leaves voxel `cell` of `volume`, in metres.
float voxelLeaves(VoxelVolume volume, int3 cell, float3 from, float3 d)
{
    float3 o = (from - volume.corner) / volume.voxel;
    float3 dd = select(abs(d) < 1e-7, select(d < 0.0, -1e-7, 1e-7), d);
    float3 far = (float3(cell) + select(dd > 0.0, 1.0, 0.0) - o) / dd;
    return min(min(far.x, far.y), far.z) * volume.voxel;
}

// The nearest voxel along the ray that is not water, as voxelsPlainTrace finds the nearest of any (from a voxel
// `nearVoxel` metres across, none for others), walking on through at most `most` voxels of water; how far the ray went
// from the first water it met to it, in metres (0 where it met none); and the first water met. Where the ray goes on
// through water past those, it meets nothing.
bool voxelsTraceDry(VoxelVolumes* volumes, float3 from, float3 d, float start, float reach, float foot0, float footScale, uint most, float nearVoxel, out VoxelHit nearest, out float wet, out VoxelHit water)
{
    wet = 0.0;
    water = (VoxelHit)0;
    water.metres = -1.0;
    for (uint k = 0; k <= most; k++)
    {
        if (!voxelsPlainTrace(volumes, from, d, start, reach, foot0, footScale, nearVoxel, nearest))
        {
            wet = water.metres >= 0.0 ? reach - water.metres : 0.0;
            return false;
        }
        VoxelVolume volume = volumes->volumes[nearest.volume];
        if (!voxelWaterOf(volume, nearest))
        {
            wet = water.metres >= 0.0 ? nearest.metres - water.metres : 0.0;
            return true;
        }
        if (water.metres < 0.0)
        {
            water = nearest;
        }
        if (nearest.level != 0)
        {
            break;
        }
        start = max(voxelLeaves(volume, nearest.cell, from, d), nearest.metres) + volume.voxel * 0.01;
    }
    wet = water.metres >= 0.0 ? reach - water.metres : 0.0;
    return false;
}

// A smooth number from 0 to 1 over the plane, a cell a unit wide, the same anywhere it is asked.
float voxelWaterNoise(float2 p)
{
    float2 i = floor(p);
    float2 f = p - i;
    f = f * f * (3.0 - 2.0 * f);
    float4 corners;
    for (uint k = 0; k < 4; k++)
    {
        int2 c = int2(i) + int2(int(k & 1u), int(k >> 1));
        uint h = uint(c.x) * 0x8DA6B343u ^ uint(c.y) * 0xD8163841u;
        h ^= h >> 16;
        h *= 0x7FEB352Du;
        h ^= h >> 15;
        corners[k] = float(h >> 8) / 16777216.0;
    }
    return lerp(lerp(corners.x, corners.y, f.x), lerp(corners.z, corners.w, f.x), f.y);
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
    float4 nearPoint = transform(frame->eyeRays, float4(ndc, 1.0, 1.0));
    float4 farther = transform(frame->eyeRays, float4(ndc, 0.5, 1.0));
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

// Where the ray through `pixel` from the camera starts, as the coarse pass says it may, and how large a pixel is along
// it: false where the coarse rays round it met nothing.
bool voxelPixelStart(Frame* frame, float2 pixel, out float3 d, out float start, out float footScale)
{
    VoxelVolumes* volumes = voxelVolumes();
    d = voxelRay(frame, pixel);
    footScale = voxelPixelAngle(frame, pixel) * volumes->lodScale;
    start = 0.0;
    if (volumes->coarseStep != 0)
    {
        uint2 c = min(uint2(pixel) / volumes->coarseStep, volumes->coarseSize - 2);
        uint w = volumes->coarseSize.x;
        float* t = volumes->coarse;
        start = min(min(t[c.y * w + c.x], t[c.y * w + c.x + 1]), min(t[(c.y + 1) * w + c.x], t[(c.y + 1) * w + c.x + 1]));
    }
    return start < 1e9;
}

// The ray from the camera through `pixel` to the nearest voxel, starting where the coarse pass says it may.
bool voxelPixel(Frame* frame, float2 pixel, out float3 d, out VoxelHit hit)
{
    hit = (VoxelHit)0;
    hit.glassT = -1.0;
    float start;
    float footScale;
    return voxelPixelStart(frame, pixel, d, start, footScale)
        && voxelsTrace(voxelVolumes(), voxelOrigin(frame, pixel), d, start, 1e9, 0.0, footScale, hit);
}

// The same ray walked by the plain walk, as the lit pass walks it where no depth pass kept what a pixel met: the view
// then has no rings (src/passes.rs).
bool voxelPlainPixel(Frame* frame, float2 pixel, out float3 d, out VoxelHit hit)
{
    hit = (VoxelHit)0;
    hit.glassT = -1.0;
    float start;
    float footScale;
    return voxelPixelStart(frame, pixel, d, start, footScale)
        && voxelsPlainTrace(voxelVolumes(), voxelOrigin(frame, pixel), d, start, 1e9, 0.0, footScale, 0.0, hit);
}

// What the probe reads (Renderer::voxels_on_gpu), at push.buffers: a volume, and `count` cells of it, x, y and z a
// word each, from its window's least sector for an endless one; and where it writes what each holds: its palette
// place and one, or 0 for nothing.
struct VoxelProbe
{
    VoxelVolume volume;
    int* cells;
    uint* found;
    uint count;
    uint pad;
};

// The voxels a volume holds at cells, for checking the GPU's voxels against the CPU's: a thread a cell.
[shader("compute")]
[numthreads(64, 1, 1)]
void voxelProbe(uint3 id: SV_DispatchThreadID)
{
    VoxelProbe* probe = (VoxelProbe*)push.buffers;
    if (id.x >= probe->count)
    {
        return;
    }
    VoxelVolume volume = probe->volume;
    int3 cell = int3(probe->cells[id.x * 3], probe->cells[id.x * 3 + 1], probe->cells[id.x * 3 + 2]);
    int brick = voxelBrickAt(volume, cell >> 3);
    uint found = 0;
    int3 l = cell & 7;
    uint index = uint(l.x + 8 * (l.y + 8 * l.z));
    uint* bits = volume.bricks + uint(max(brick, 0)) * VOXEL_BRICK_WORDS;
    if (brick >= 0 && voxelBit(bits, index))
    {
        uint before = countbits(bits[index >> 5] & ((1u << (index & 31u)) - 1u));
        for (uint k = 0; k < (index >> 5); k++)
        {
            before += countbits(bits[k]);
        }
        uint at = bits[18] + before;
        found = voxelPlace(bits, (volume.materials[at >> 2] >> ((at & 3u) * 8u)) & 255u) + 1u;
    }
    probe->found[id.x] = found;
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
    // A footprint of eight voxels or more stops each walk at the first brick with anything in it. Each pixel's ray starts
    // a brick and a half short of it, in the bricks of the volume met, as a ring's bricks are larger.
    bool found = voxelsTrace(volumes, voxelOrigin(frame, pixel), d, 0.0, 1e9, 1e6, 0.0, hit);
    float voxel = found ? volumes->volumes[hit.volume].voxel : 0.0;
    volumes->coarse[id.y * volumes->coarseSize.x + id.x] = found ? max(hit.metres - 12.0 * voxel, 0.0) : 1e10;
}

// How far out from a voxel its shadow ray starts, in its voxels, by the level it was met at: a coarser level's ray
// starts a group or a brick out, in the cell the view's ray came through, so it does not shadow itself.
float voxelLift(uint level)
{
    return level == 0 ? 0.02 : (level == 1 ? 2.02 : 8.02);
}

// Whether a voxel stands between `world`, `metres` from the camera through `pixel`, and the sun, its ray started
// `lift` metres out along `normal` and walked at the level the view's ray is; from a voxel `nearVoxel` metres across,
// 0 for anything else (voxelsPlainTrace).
bool voxelSunBlocked(Frame* frame, VoxelVolumes* volumes, float2 pixel, float3 world, float3 normal, float metres, float lift, float nearVoxel)
{
    float foot = metres * voxelPixelAngle(frame, pixel) * volumes->lodScale;
    // Water of voxels lets the sun through to what is under it.
    VoxelHit blocked;
    VoxelHit wetHit;
    float wetThrough;
    float3 from = world + normal * lift;
    return voxelsTraceDry(volumes, from, -frame->sunDirection, 0.0, VOXEL_SHADOW_REACH, foot, 0.0, 3, nearVoxel, blocked, wetThrough, wetHit);
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
    bool met = voxelPixel(frame, input.position.xy, d, hit);
    VoxelVolumes* volumes = voxelVolumes();
    // The glass the ray passed through, kept for the glass pass, even over the sky.
    if (volumes->glassOn != 0u && hit.glassT >= 0.0)
    {
        uint2 at = uint2(input.position.xy);
        uint side = uint(hit.glassAxis + 1) | (hit.glassVolume << 4);
        volumes->glass[at.y * volumes->seenWidth + at.x] =
            uint4(asuint(hit.glassMetres), side, hit.glassColour, frame->frameIndex + 1);
    }
    if (!met)
    {
        discard;
    }
    VoxelVolume volume = volumes->volumes[hit.volume];
    float3 normal = voxelNormal(hit, d);
    float roughness = smoothedRoughness(frame, normal, volume.roughness);
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

// How far the sun's light is looked for towards the sun from water of voxels, in metres, and how many times coarser
// than its pixel the walk goes: the water's own shadow, cheaply.
static const float VOXEL_WATER_SHADOW_REACH = 24.0;
static const float VOXEL_WATER_SHADOW_COARSER = 3.0;

// A voxel of water the view meets at `hit`, seen through pixel `pixel` along `d`. Its colour is its shade, as deep as
// the water is over the ground there (src/voxel_volume/rivers.rs), lit by the sky and the sun as the body of the water
// scatters it; over that it reflects the sky and what stands round it, glints, and ripples drifting across it. The
// sides of its steps, where its top drops a voxel, are lit as its top, so a sloping river reads as one surface. Foam
// where it meets the bank, coming and going.
VoxelOut voxelWaterLit(Frame* frame, float2 pixel, float3 d, VoxelHit hit)
{
    VoxelVolumes* volumes = voxelVolumes();
    VoxelVolume volume = volumes->volumes[hit.volume];
    float3 from = voxelOrigin(frame, pixel);
    float3 world = from + d * hit.metres;
    float3 face = voxelNormal(hit, d);
    float3 l = -frame->sunDirection;
    bool atmosphere = frame->lighting.sky == SKY_ATMOSPHERE;
    float3 sunLight = atmosphere ? sunThroughAtmosphere(frame) : frame->sunColor;
    float footScale = voxelPixelAngle(frame, pixel) * volumes->lodScale;
    // A step's side: open in front of it, and water under what is in front.
    bool top = face.y > 0.5;
    if (!top && hit.level == 0 && abs(face.y) < 0.5)
    {
        int3 ahead = hit.cell + int3(face);
        top = voxelKindAt(volume, ahead) == 0u && voxelKindAt(volume, ahead - int3(0, 1, 0)) == 1u;
    }
    float3 lightFace = top ? float3(0.0, 1.0, 0.0) : face;
    float shadow = sunShadow(frame, world, lightFace);
    if (shadow > 0.0)
    {
        VoxelHit blocked;
        VoxelHit wetHit;
        float wetThrough;
        float3 off = world + face * (hit.level == 0 ? 0.02 : 2.02) * volume.voxel;
        float foot = hit.metres * footScale * VOXEL_WATER_SHADOW_COARSER;
        if (voxelsTraceDry(volumes, off, l, 0.0, VOXEL_WATER_SHADOW_REACH, foot, footScale * VOXEL_WATER_SHADOW_COARSER, 2, volume.voxel, blocked, wetThrough, wetHit))
        {
            shadow = 0.0;
        }
    }
    float t = frame->time;
    float3 normal = lightFace;
    if (top)
    {
        // Ripples drifting across it smoothly, whatever voxels they cross.
        float2 slope = float2(0.0);
        for (uint k = 0; k < 3; k++)
        {
            float size = 0.9 / float(1u << k);
            float2 p = world.xz / size + float2(t * 0.35, t * 0.22) * float(k + 1) + float(k) * 17.0;
            float e = 0.05;
            float c = voxelWaterNoise(p);
            slope += float2(voxelWaterNoise(p + float2(e, 0.0)) - c, voxelWaterNoise(p + float2(0.0, e)) - c) / e * (0.05 / float(k + 1));
        }
        normal = normalize(float3(-slope.x, 1.0, -slope.y));
    }
    float3 body = voxelColour(volume, hit);
    Surface s;
    s.normal = normal;
    s.view = -d;
    s.roughness = 0.06;
    s.diffuse = float3(0.0);
    s.f0 = float3(0.02);
    float3 ambient = ambientLight(frame, world, s, sunLight, atmosphere);
    float nv = saturate(dot(normal, -d));
    float fresnel = 0.02 + 0.98 * pow(1.0 - nv, 5.0);
    float found;
    float3 reflected = environmentReflection(frame, pixel, world, normal, reflect(d, normal), s.roughness, ambient, atmosphere, found);
    float3 under = body * (ambient * 0.8 + sunLight * shadow * saturate(l.y) * 0.5 / PI);
    float3 color = under * (1.0 - fresnel) + reflected * fresnel;
    Surface glint = s;
    glint.roughness = 0.12;
    color += shade(glint, l, sunLight) * shadow;
    // Churned where a waterfall lands: its last two shades, white foam coming and going over it.
    if (hit.level == 0)
    {
        uint material = voxelMaterialOf(volume, hit.brick, hit.cell);
        if (material + 3u >= volume.water + volume.shades)
        {
            float boil = voxelWaterNoise(float2(hit.cell.xz) * 0.6 + float2(t * 1.7, -t * 1.3) + float(hit.cell.y) * 0.3);
            float3 foamLit = 0.9 * (ambient + sunLight * shadow * saturate(l.y + 0.3) / PI);
            color = lerp(color, foamLit, smoothstep(0.2, 0.55, boil) * (material + 2u == volume.water + volume.shades ? 1.0 : 0.6));
        }
    }
    if (hit.level == 0 && top)
    {
        float edges = 0.0;
        for (uint k = 0; k < 4; k++)
        {
            int3 o = k == 0 ? int3(1, 0, 0) : (k == 1 ? int3(-1, 0, 0) : (k == 2 ? int3(0, 0, 1) : int3(0, 0, -1)));
            edges += voxelKindAt(volume, hit.cell + o) == 2u ? 1.0 : 0.0;
        }
        float churn = voxelWaterNoise(float2(hit.cell.xz) * 0.45 + float2(t * 0.6, -t * 0.45));
        float foam = saturate(edges * 0.5) * step(0.4, churn);
        float3 foamLit = 0.85 * (ambient + sunLight * shadow * saturate(l.y + 0.3) / PI);
        color = lerp(color, foamLit, foam);
    }
    if (atmosphere)
    {
        float4 haze = aerialPerspectiveAt(frame, pixel / float2(frame->viewport), hit.metres);
        color = color * haze.a + haze.rgb * skyLook(frame);
    }
    color = cloudsBefore(frame, pixel / float2(frame->viewport), world, color);
    VoxelOut o;
    o.color = float4(color, 1.0);
    o.velocity = motion(transform(frame->unjitteredViewProj, float4(world, 1.0)), transform(frame->prevViewProj, float4(world, 1.0)));
    o.depth = voxelDepthOf(frame, world);
    return o;
}

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
    else if (!voxelPlainPixel(frame, pixel, d, hit))
    {
        discard;
    }
    VoxelVolume volume = volumes->volumes[hit.volume];
    if (voxelWaterOf(volume, hit))
    {
        return voxelWaterLit(frame, pixel, d, hit);
    }
    float3 world = voxelOrigin(frame, pixel) + d * hit.metres;
    float3 normal = voxelNormal(hit, d);
    float3 albedo = voxelColour(volume, hit);
    // A voxel that glows, as a lantern's flame does, shines with its own light.
    float glows = 0.0;
    if (hit.level == 0u && hit.brick >= 0)
    {
        uint* bits = volume.bricks + uint(hit.brick) * VOXEL_BRICK_WORDS;
        if ((bits[19] & VOXEL_GLOWS) != 0u)
        {
            glows = float(voxelPlaceKind(volume, voxelPlaceAt(volume, bits, hit.cell & 7)) >> 4) / 15.0;
        }
    }

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
        && voxelSunBlocked(frame, volumes, pixel, world, normal, hit.metres, voxelLift(hit.level) * volume.voxel, volume.voxel))
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
    // Else by the light kept round the camera for the voxels: their traced shadows keep the sun out.
    else
    {
        blockLitHere = voxelLitAround(world, normal, blockLit, blockGlow);
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
    // What it reflects of its surroundings is as dark as the light kept round it leaves it, as a block's face is.
    if (blockLitHere)
    {
        float3 round = reflected * blockSkyShare(blockLit.x, blockGlow.w) + blockGlow.rgb * blockGlowShare(blockLit.y);
        reflected = lerp(round, reflected, found);
    }
    color += ambient * s.diffuse * occlusion.diffuse;
    color += reflected * ambientFresnel * (1.0 - s.roughness * 0.7) * specularOcclusion;
    color += albedo * glows * glows * VOXEL_GLOW_SHINE;

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

// ---------------------------------------------------------------- See-through voxels

// How rough glass is.
static const float VOXEL_GLASS_ROUGHNESS = 0.06;

struct VoxelGlassOut
{
    float4 color : SV_Target0;
    float depth : SV_Depth;
};

// The see-through voxels in view, over everything opaque drawn behind them, as the depth pass found them: the first
// each pixel's ray passed through, lit as a voxel is, and reflecting what is round it, in front of what is behind it
// as much as it is solid. Drawn with its colour multiplied by how much of what is behind it hides, so it blends over
// what is there.
[shader("fragment")]
VoxelGlassOut voxelGlass(VoxelVertex input)
{
    Frame* frame = push.frame;
    float2 pixel = input.position.xy;
    VoxelVolumes* volumes = voxelVolumes();
    uint2 at = uint2(pixel);
    uint4 glass = volumes->glassOn != 0u ? volumes->glass[at.y * volumes->seenWidth + at.x] : uint4(0);
    if (glass.w != frame->frameIndex + 1)
    {
        discard;
    }
    float metres = asfloat(glass.x);
    int axis = int(glass.y & 3u) - 1;
    VoxelVolume volume = volumes->volumes[min(glass.y >> 4, volumes->count - 1)];
    uint packed = glass.z;
    float3 d = voxelRay(frame, pixel);
    float3 world = voxelOrigin(frame, pixel) + d * metres;
    float3 normal = -d;
    if (axis >= 0)
    {
        normal = 0.0;
        normal[axis] = d[axis] > 0.0 ? -1.0 : 1.0;
    }
    float3 albedo = pow(float3(packed & 255u, (packed >> 8) & 255u, (packed >> 16) & 255u) / 255.0, 2.2);
    float solid = 1.0 - float(voxelKind(packed).x) / 16.0;

    Surface s;
    s.normal = normal;
    s.view = -d;
    s.roughness = VOXEL_GLASS_ROUGHNESS;
    s.diffuse = albedo;
    s.f0 = float3(0.04);
    bool atmosphere = frame->lighting.sky == SKY_ATMOSPHERE;
    float3 sunLight = atmosphere ? sunThroughAtmosphere(frame) : frame->sunColor;
    float shadow = dot(normal, -frame->sunDirection) > 0.0 ? sunShadow(frame, world, normal) : 0.0;
    if (shadow > 0.0 && voxelSunBlocked(frame, volumes, pixel, world, normal, metres, 0.02 * volume.voxel, volume.voxel))
    {
        shadow = 0.0;
    }
    float2 lit = float2(1.0, 0.0);
    float4 glow = 0.0;
    bool litHere = voxelLitAround(world, normal, lit, glow);
    float3 ambient = ambientLight(frame, world, s, sunLight, atmosphere);
    if (litHere)
    {
        ambient = ambient * blockSkyShare(lit.x, glow.w) + glow.rgb * blockGlowShare(lit.y);
    }
    // Its own body, as much as it is solid; then what it reflects, the sun's glint and its surroundings.
    Surface body = s;
    body.f0 = 0.0;
    float3 color = (shade(body, -frame->sunDirection, sunLight) * shadow + ambient * albedo) * solid;
    Surface shine = s;
    shine.diffuse = 0.0;
    color += shade(shine, -frame->sunDirection, sunLight) * shadow;
    float found;
    float3 reflected = environmentReflection(frame, pixel, world, normal, reflect(d, normal), s.roughness, ambient, atmosphere, found);
    if (litHere)
    {
        float3 round = reflected * blockSkyShare(lit.x, glow.w) + glow.rgb * blockGlowShare(lit.y);
        reflected = lerp(round, reflected, found);
    }
    float nv = saturate(dot(normal, -d));
    float fresnel = 0.04 + 0.96 * pow(1.0 - nv, 5.0);
    color += reflected * fresnel;
    // What it hides of what is behind it: as much as it is solid, and what it reflects.
    float hides = saturate(solid + fresnel * (1.0 - solid));
    if (atmosphere)
    {
        float4 haze = aerialPerspectiveAt(frame, pixel / float2(frame->viewport), metres);
        color = color * haze.a + haze.rgb * skyLook(frame) * hides;
    }
    VoxelGlassOut o;
    o.color = float4(color, hides);
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
    float near = 0.0;
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
            near = voxel;
        }
    }
    if (dot(normal, -frame->sunDirection) <= 0.0)
    {
        return 1.0;
    }
    return voxelSunBlocked(frame, volumes, pixel, world, normal, metres, lift, near) ? 0.0 : 1.0;
}

// ---------------------------------------------------------------- The light kept round the camera

// Light from the sky and from glowing voxels, kept round the camera a cell a brick (src/voxel_volume/light.rs), as the
// light kept for blocks is a byte a block: the sky's falls straight down through open air, and both spread a step less
// a cell sideways, up, and through what lets only some through, so a cellar and a room under a roof are dark but for
// what comes in at their doors and windows and their lanterns. What lets light through a cell is worked out from its
// brick's voxels each frame (voxelLightCells), then the light is spread a few steps a frame (voxelLightSpread), from
// last frame's moved as the camera moved.

// The glow of a level, a fifteenth of the most for a level as blocks' light, falls a level a metre: a thirtieth a cell.
static const float VOXEL_LIGHT_STEP = 1.0 / 30.0;
// Its second word, over its glow's colour: how brightly it glows, and whether the sky is open over it, for the top row.
// And whether the sky reaches it straight down, open all the way (voxelLightSky).
static const uint VOXEL_CELL_GLOW_SHIFT = 16u;
static const uint VOXEL_CELL_SKY = 1u << 20;
static const uint VOXEL_CELL_OPEN_SKY = 1u << 21;

struct VoxelLightGrid
{
    // The volumes, at their own places.
    VoxelVolumes* volumes;
    uint* lightA;
    uint* lightB;
    uint2* cells;
    float3 corner;
    float cell;
    int3 size;
    uint pad;
    // Where cell 0 was last frame, less where it is now, in cells; and whether last frame's light is to be read.
    int3 moved;
    uint kept;
};

VoxelLightGrid* voxelLightGrid()
{
    return (VoxelLightGrid*)(uint64_t(push.textures.x) | (uint64_t(push.textures.y) << 32));
}

// Whether a ring of an endless volume leaves the column at `middle` to another: in its hole, which the finer ring
// draws, or past its outer box.
bool voxelRingLeaves(VoxelVolume volume, float3 middle)
{
    float2 at = (middle.xz - volume.corner.xz) / volume.voxel;
    bool inHole = volume.hole.z > volume.hole.x && all(at > volume.hole.xy) && all(at < volume.hole.zw);
    bool inside = volume.outer.z <= volume.outer.x || (all(at > volume.outer.xy) && all(at < volume.outer.zw));
    return inHole || !inside;
}

// The voxels of the cell round `middle`, of all the volumes, as one brick's bits, see-through voxels left out: none
// where no volume has a brick there. A volume keeps only the voxels of its ground that open air is beside, so what is
// past the sides of a volume's window or under its floor, with no volume there, is taken as solid, all its bits set;
// over the volumes is open sky. With `glows`, the brightest glow of its voxels and that voxel's colour.
void voxelCellBits(VoxelVolumes* volumes, float3 middle, bool glows, out uint bits[16], out uint glow, out uint3 colour)
{
    for (uint w = 0; w < 16; w++)
    {
        bits[w] = 0u;
    }
    glow = 0;
    colour = 0;
    bool inside = false;
    bool under = false;
    for (uint v = 0; v < volumes->count; v++)
    {
        VoxelVolume volume = volumes->volumes[v];
        if (voxelRingLeaves(volume, middle))
        {
            continue;
        }
        int3 b = int3(floor((middle - volume.corner) / (volume.voxel * 8.0)));
        int brick = voxelBrickAt(volume, b);
        if (brick == -2)
        {
            under = under || b.y < volume.sectorCount.y * 8;
            continue;
        }
        inside = true;
        if (brick < 0)
        {
            continue;
        }
        uint* words = volume.bricks + uint(brick) * VOXEL_BRICK_WORDS;
        uint marks = words[19];
        if ((marks & (VOXEL_GLOWS | VOXEL_SEE_THROUGH | VOXEL_WET)) == 0u)
        {
            for (uint w = 0; w < 16; w++)
            {
                bits[w] |= words[w];
            }
            continue;
        }
        // Glass and water let light through, and a glowing voxel gives it out and lets it through, as a block of light
        // does: their voxels looked at one by one.
        uint at = words[18];
        for (uint w = 0; w < 16; w++)
        {
            uint left = words[w];
            uint kept = left;
            while (left != 0u)
            {
                uint bit = firstbitlow(left);
                left &= left - 1u;
                uint place = voxelPlace(words, (volume.materials[at >> 2] >> ((at & 3u) * 8u)) & 255u);
                at++;
                uint packed = volume.palette[place];
                uint2 kind = voxelKind((packed & 0xFFFFFFu) | (voxelPlaceKind(volume, place) << 24));
                if (kind.x != 0u || kind.y != 0u || voxelIsWater(volume, place))
                {
                    kept &= ~(1u << bit);
                }
                if (glows && kind.y > glow)
                {
                    glow = kind.y;
                    colour = uint3(packed & 255u, (packed >> 8) & 255u, (packed >> 16) & 255u);
                }
            }
            bits[w] |= kept;
        }
    }
    if (!inside && under)
    {
        for (uint w = 0; w < 16; w++)
        {
            bits[w] = 0xFFFFFFFFu;
        }
    }
}

// Which of the 64 lines from side `side` of a brick to its middle, half its width, meet none of its voxels `bits`, a
// bit a line, the sides in the order +x, -x, +y, -y, +z, -z: lines along x by their y and z, along y by their x and z,
// along z by their x and y, each first plus eight times the second.
uint2 voxelOpenHalf(uint bits[16], uint side)
{
    uint2 open = 0u;
    if (side < 2u)
    {
        // A row of eight along x is a byte, its low four the -x half.
        uint half = side == 0u ? 0xF0u : 0x0Fu;
        for (uint w = 0; w < 16; w++)
        {
            for (uint b = 0; b < 4; b++)
            {
                uint line = w * 4u + b;
                bool clear = ((bits[w] >> (b * 8u)) & half) == 0u;
                open[line >> 5] |= clear ? 1u << (line & 31u) : 0u;
            }
        }
    }
    else if (side < 4u)
    {
        // The four rows of a word are the lower or upper half of one z.
        for (uint z = 0; z < 8; z++)
        {
            uint v = bits[z * 2u + (side == 2u ? 1u : 0u)];
            uint rows = (v | (v >> 8) | (v >> 16) | (v >> 24)) & 0xFFu;
            uint line = z * 8u;
            open[line >> 5] |= (~rows & 0xFFu) << (line & 31u);
        }
    }
    else
    {
        // The words of the lower four z, or of the upper four, or'd together: the even ones the lower rows of y.
        uint from = side == 4u ? 8u : 0u;
        uint2 any = 0u;
        for (uint w = 0; w < 8; w++)
        {
            any[w & 1u] |= bits[from + w];
        }
        open = ~any;
    }
    return open;
}

// Each cell of the light kept round the camera: how much light passes between it and the cell after it along x, y and
// z, by the lines open from the middle of one to the middle of the other; how much of it is open; how brightly and in
// what colour its voxels glow; and for the top row, whether the sky is open over it.
[shader("compute")]
[numthreads(4, 4, 4)]
void voxelLightCells(uint3 id: SV_DispatchThreadID)
{
    VoxelLightGrid* grid = voxelLightGrid();
    if (any(int3(id) >= grid->size))
    {
        return;
    }
    VoxelVolumes* volumes = grid->volumes;
    float3 middle = grid->corner + (float3(id) + 0.5) * grid->cell;
    uint own[16];
    uint glow;
    uint3 glowColour;
    voxelCellBits(volumes, middle, true, own, glow, glowColour);
    uint solid = 0;
    for (uint w = 0; w < 16; w++)
    {
        solid += countbits(own[w]);
    }
    uint first = 0;
    bool closed = true;
    for (uint axis = 0; axis < 3; axis++)
    {
        float3 step = 0.0;
        step[axis] = grid->cell;
        uint next[16];
        uint nGlow;
        uint3 nColour;
        voxelCellBits(volumes, middle + step, false, next, nGlow, nColour);
        uint2 through = voxelOpenHalf(own, axis * 2u) & voxelOpenHalf(next, axis * 2u + 1u);
        uint lines = countbits(through.x) + countbits(through.y);
        first |= ((lines * 15u + 63u) / 64u) << (axis * 4u);
        // Open from its middle to any side, by its own halves: else it is solid.
        closed = closed && all(voxelOpenHalf(own, axis * 2u) == 0u) && all(voxelOpenHalf(own, axis * 2u + 1u) == 0u);
    }
    uint open = closed ? 0u : max((15u * (512u - solid) + 511u) / 512u, 1u);
    first |= open << VOXEL_CELL_OPEN_SHIFT;
    uint second = glow << VOXEL_CELL_GLOW_SHIFT;
    // The top row: open to the sky where no brick stands over the cell above it, up to the volumes' tops.
    if (int(id.y) == grid->size.y - 1)
    {
        bool sky = true;
        for (uint v = 0; v < volumes->count && sky; v++)
        {
            VoxelVolume volume = volumes->volumes[v];
            if (voxelRingLeaves(volume, middle))
            {
                continue;
            }
            int3 b = int3(floor((middle - volume.corner) / (volume.voxel * 8.0)));
            for (int y = max(b.y + 2, 0); y < volume.sectorCount.y * 8 && sky; y++)
            {
                int found = voxelBrickAt(volume, int3(b.x, y, b.z));
                sky = found == -1;
                if (found == -2)
                {
                    sky = true;
                    break;
                }
            }
        }
        second |= sky ? VOXEL_CELL_SKY : 0u;
    }
    // Its glow's colour as light, bright as its brightest channel, in 5, 6 and 5 bits.
    float3 linear = pow(float3(glowColour) / 255.0, 2.2);
    linear /= max(max(linear.r, linear.g), max(linear.b, 1e-4));
    uint hue = (uint(linear.r * 31.0 + 0.5) << 11) | (uint(linear.g * 63.0 + 0.5) << 5) | uint(linear.b * 31.0 + 0.5);
    uint index = uint((int(id.y) * grid->size.z + int(id.z)) * grid->size.x + int(id.x));
    grid->cells[index] = uint2(first, second | hue);
}

// Whether the sky reaches each cell straight down, from over the top row where it is open, through every side between
// two cells open all the way: a thread a column, from the top down, afresh each frame.
[shader("compute")]
[numthreads(8, 1, 8)]
void voxelLightSky(uint3 id: SV_DispatchThreadID)
{
    VoxelLightGrid* grid = voxelLightGrid();
    int3 size = grid->size;
    if (int(id.x) >= size.x || int(id.z) >= size.z)
    {
        return;
    }
    uint layer = uint(size.x * size.z);
    uint index = uint(size.y - 1) * layer + uint(int(id.z) * size.x + int(id.x));
    bool open = (grid->cells[index].y & VOXEL_CELL_SKY) != 0u;
    for (int y = size.y - 1; y >= 0; y--)
    {
        uint2 cell = grid->cells[index];
        // Into it from the one over it.
        open = open && ((cell.x >> 4) & 15u) == 15u;
        if (open)
        {
            grid->cells[index].y = cell.y | VOXEL_CELL_OPEN_SKY;
        }
        index -= layer;
    }
}

// One step of spreading the light kept round the camera: each cell the most of its neighbours' less a step, all the
// sky's where it reaches it straight down, and each cell's own glow. Read from push.level's light, written to the
// other; with push.list, read from where last frame's cells were.
[shader("compute")]
[numthreads(4, 4, 4)]
void voxelLightSpread(uint3 id: SV_DispatchThreadID)
{
    VoxelLightGrid* grid = voxelLightGrid();
    int3 size = grid->size;
    if (any(int3(id) >= size))
    {
        return;
    }
    uint* from = push.level == 0 ? grid->lightA : grid->lightB;
    uint* to = push.level == 0 ? grid->lightB : grid->lightA;
    int3 c = int3(id);
    uint index = uint((c.y * size.z + c.z) * size.x + c.x);
    uint2 own = grid->cells[index];
    if (((own.x >> VOXEL_CELL_OPEN_SHIFT) & 15u) == 0u)
    {
        to[index] = 0u;
        return;
    }
    int3 moved = push.list != 0 ? grid->moved : int3(0);
    bool kept = push.list == 0 || grid->kept != 0u;
    float sky = (own.y & VOXEL_CELL_OPEN_SKY) != 0u ? 1.0 : 0.0;
    float glow = float((own.y >> VOXEL_CELL_GLOW_SHIFT) & 15u) / 15.0;
    uint hue = own.y & 0xFFFFu;
    for (uint k = 0; k < 6; k++)
    {
        // Each side, +x, -x, +y, -y, +z, -z: how much passes to the cell after is kept with the cell before.
        uint axis = k / 2u;
        int3 n = c;
        n[axis] += (k & 1u) == 0u ? 1 : -1;
        if (any(n < int3(0)) || any(n >= size))
        {
            continue;
        }
        uint nIndex = uint((n.y * size.z + n.z) * size.x + n.x);
        uint before = (k & 1u) == 0u ? own.x : grid->cells[nIndex].x;
        float pass = float((before >> (axis * 4u)) & 15u) / 15.0;
        if (pass <= 0.0)
        {
            continue;
        }
        float nSky = 0.0;
        float nGlow = 0.0;
        uint nHue = 0;
        int3 o = n + moved;
        if (kept && all(o >= int3(0)) && all(o < size))
        {
            uint light = from[uint((o.y * size.z + o.z) * size.x + o.x)];
            nSky = float(light & 255u) / 255.0;
            nGlow = float((light >> 8) & 255u) / 255.0;
            nHue = light >> 16;
        }
        // Light through what lets only some of it through loses more on the way.
        float lost = VOXEL_LIGHT_STEP * (1.0 + 2.0 * (1.0 - pass));
        sky = max(sky, nSky - lost);
        float g = nGlow - lost;
        if (g > glow)
        {
            glow = g;
            hue = nHue;
        }
    }
    uint s = uint(saturate(sky) * 255.0 + 0.5);
    uint g = uint(saturate(glow) * 255.0 + 0.5);
    to[index] = s | (g << 8) | ((g > 0u ? hue : 0u) << 16);
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
    // Water of voxels is no ground: the floor is walked on through it.
    VoxelHit hit;
    VoxelHit wetHit;
    float wetThrough;
    if (!voxelsTraceDry(voxelVolumes(), from, d, 0.0, top - bottom, foot, 0.0, 4, 0.0, hit, wetThrough, wetHit))
    {
        discard;
    }
    float y = under ? bottom + hit.metres : top - hit.metres;
    return saturate((y - level + VOXEL_FLOOR_REACH) / (VOXEL_FLOOR_REACH + VOXEL_FLOOR_CEILING));
}
