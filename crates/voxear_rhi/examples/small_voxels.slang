// A world of small voxels traced on the GPU, for measuring (examples/small_voxels.rs): no meshes, one thread a
// pixel walks its ray through a three-level grid.
//
// - A sector is 8x8x8 bricks (64 voxels a side). Its 512 bits say which bricks it has, and the bricks it has are
//   kept one after another, so a brick's place is the sector's first plus how many bits come before its own.
// - A brick is 8x8x8 voxels. Its 512 bits say which voxels are solid, its 64 bits which 2x2x2 groups have any, and
//   its solid voxels' materials are kept one byte each, in the order of its bits.
//
// The walk steps over a whole empty sector, a whole empty brick, an empty group or an empty voxel at a time. Far
// off, where a voxel is smaller than a pixel, it stops at groups, and further still at whole bricks drawn in their
// average colour: the levels of detail cost nothing to keep, as both are in a brick already.

struct World
{
    uint* sectors;
    uint* bricks;
    uint* materials;
    uint* palette;
    int3 sectorCount;
    float voxel;
};

struct View
{
    float3 eye;
    float pixelAngle;
    float3 forward;
    float tanHalf;
    float3 right;
    float aspect;
    float3 up;
    uint flags;
    float3 sun;
    float lodScale;
    uint2 size;
    uint* output;
};

struct Push
{
    World* world;
    View* view;
};

[[vk::push_constant]] ConstantBuffer<Push> push;

static const uint SECTOR_WORDS = 25;
static const uint BRICK_WORDS = 20;
static const uint EMPTY = 0xFFFFFFFFu;
static const int MOST_STEPS = 1024;
// How much coarser each step makes the levels a ray stops at: a ray that skims the grass for hundreds of voxels
// goes on in groups and then bricks, rather than running out of steps and showing the sky.
static const float STEP_COARSENING = 0.01;

static const uint SHADOWS = 1u;
static const uint OCCLUSION = 2u;
static const uint SHOW_STEPS = 4u;

struct Hit
{
    float t;
    int axis;
    int3 cell;
    int brick;
    uint level;
    int steps;
};

bool bitOf(uint* words, uint index)
{
    return ((words[index >> 5] >> (index & 31u)) & 1u) != 0u;
}

// The brick at brick cell `b`: its number, -1 for none, -2 outside the world->
int brickAt(World* world, int3 b)
{
    int3 s = b >> 3;
    if (any(s < 0) || any(s >= world->sectorCount))
    {
        return -2;
    }
    uint* sector = world->sectors + uint((s.y * world->sectorCount.z + s.z) * world->sectorCount.x + s.x) * SECTOR_WORDS;
    if (sector[24] == EMPTY)
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
    // How many bricks come before it: kept per word as 16-bit counts, plus those below it in its own word.
    uint before = (sector[16 + (index >> 6)] >> (((index >> 5) & 1u) * 16u)) & 0xFFFFu;
    before += countbits(word & ((1u << bit) - 1u));
    return int(sector[24] + before);
}

bool solidAt(World* world, int3 cell)
{
    int brick = brickAt(world, cell >> 3);
    if (brick < 0)
    {
        return false;
    }
    int3 l = cell & 7;
    return bitOf(world->bricks + uint(brick) * BRICK_WORDS, uint(l.x + 8 * (l.y + 8 * l.z)));
}

// Walks the ray from `o` along `d` (voxels) to `tMax`. A voxel is `foot0 + t * footScale` voxels to a pixel at
// distance `t`, which picks the level it stops at.
bool trace(World* world, float3 o, float3 d, float tMax, float foot0, float footScale, out Hit hit)
{
    hit = (Hit)0;
    hit.axis = -1;
    d = select(abs(d) < 1e-7, select(d < 0.0, -1e-7, 1e-7), d);
    float3 inv = 1.0 / d;
    int3 dims = world->sectorCount * 64;
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
    for (int i = 0; i < MOST_STEPS; i++)
    {
        if (t >= tExit)
        {
            hit.steps = i;
            return false;
        }
        int3 c = int3(floor(o + d * t));
        if (axis >= 0)
        {
            c[axis] = d[axis] > 0.0 ? boundary : boundary - 1;
        }
        c = clamp(c, int3(0), dims - 1);
        int size = 64;
        int brick = brickAt(world, c >> 3);
        if (brick >= 0)
        {
            size = 8;
            uint* bits = world->bricks + uint(brick) * BRICK_WORDS;
            float foot = foot0 + t * footScale + float(i) * STEP_COARSENING;
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
            if (foot >= 2.0)
            {
                int3 g = l >> 1;
                if (bitOf(bits + 16, uint(g.x + 4 * (g.y + 4 * g.z))))
                {
                    hit.level = 1;
                    return true;
                }
                size = 2;
            }
            else
            {
                if (bitOf(bits, uint(l.x + 8 * (l.y + 8 * l.z))))
                {
                    hit.level = 0;
                    return true;
                }
                size = 1;
            }
        }
        else if (brick == -1)
        {
            // An empty brick in a sector with bricks, or a whole empty sector.
            int3 s = c >> 6;
            uint* sector = world->sectors + uint((s.y * world->sectorCount.z + s.z) * world->sectorCount.x + s.x) * SECTOR_WORDS;
            size = sector[24] == EMPTY ? 64 : 8;
        }
        int3 low = c & ~(size - 1);
        float3 edge = float3(low + up * size);
        float3 te = (edge - o) * inv;
        axis = te.x < te.y && te.x < te.z ? 0 : (te.y < te.z ? 1 : 2);
        t = max(te[axis], t);
        boundary = int(edge[axis]);
    }
    hit.steps = MOST_STEPS;
    return false;
}

float3 unpackColour(uint packed)
{
    return float3(packed & 255u, (packed >> 8) & 255u, (packed >> 16) & 255u) / 255.0;
}

float3 colourOf(World* world, Hit hit)
{
    uint* bits = world->bricks + uint(hit.brick) * BRICK_WORDS;
    if (hit.level != 0)
    {
        return unpackColour(bits[19]);
    }
    int3 l = hit.cell & 7;
    uint index = uint(l.x + 8 * (l.y + 8 * l.z));
    uint before = countbits(bits[index >> 5] & ((1u << (index & 31u)) - 1u));
    for (uint k = 0; k < (index >> 5); k++)
    {
        before += countbits(bits[k]);
    }
    uint at = bits[18] + before;
    uint material = (world->materials[at >> 2] >> ((at & 3u) * 8u)) & 255u;
    return unpackColour(world->palette[material]);
}

uint hashOf(int3 c)
{
    uint h = uint(c.x) * 0x8DA6B343u ^ uint(c.y) * 0xD8163841u ^ uint(c.z) * 0xCB1AB31Fu;
    h ^= h >> 16;
    h *= 0x7FEB352Du;
    h ^= h >> 15;
    h *= 0x846CA68Bu;
    h ^= h >> 16;
    return h;
}

float corner(bool a, bool b, bool c)
{
    return (a && b) ? 0.0 : (3.0 - float(a) - float(b) - float(c)) / 3.0;
}

// How open a face's spot is: the voxels in the layer in front of it round its corners, blended across the face.
float occlusion(World* world, int3 cell, int axis, int3 n, float3 at)
{
    int u = (axis + 1) % 3;
    int v = (axis + 2) % 3;
    int3 du = int3(0);
    int3 dv = int3(0);
    du[u] = 1;
    dv[v] = 1;
    int3 q = cell + n;
    bool l = solidAt(world, q - du);
    bool r = solidAt(world, q + du);
    bool b = solidAt(world, q - dv);
    bool f = solidAt(world, q + dv);
    float lb = corner(l, b, solidAt(world, q - du - dv));
    float rb = corner(r, b, solidAt(world, q + du - dv));
    float lf = corner(l, f, solidAt(world, q - du + dv));
    float rf = corner(r, f, solidAt(world, q + du + dv));
    float2 s = float2(at[u] - float(cell[u]), at[v] - float(cell[v]));
    return lerp(lerp(lb, rb, s.x), lerp(lf, rf, s.x), s.y);
}

float valueNoise(float2 p)
{
    int2 i = int2(floor(p));
    float2 f = p - floor(p);
    f = f * f * (3.0 - 2.0 * f);
    float a = float(hashOf(int3(i, 7)) >> 8) / 16777216.0;
    float b = float(hashOf(int3(i + int2(1, 0), 7)) >> 8) / 16777216.0;
    float c = float(hashOf(int3(i + int2(0, 1), 7)) >> 8) / 16777216.0;
    float e = float(hashOf(int3(i + int2(1, 1), 7)) >> 8) / 16777216.0;
    return lerp(lerp(a, b, f.x), lerp(c, e, f.x), f.y);
}

float3 skyOf(float3 d)
{
    float h = saturate(d.y);
    float3 sky = lerp(float3(0.10, 0.12, 0.24), float3(0.015, 0.02, 0.06), sqrt(h));
    if (d.y > 0.0)
    {
        int3 q = int3(floor(d * 300.0));
        if ((hashOf(q) >> 8) > 16760000u)
        {
            sky += float3(1.6);
        }
        float2 p = d.xz / (d.y + 0.2) * 2.0;
        float cloud = valueNoise(p) * 0.5 + valueNoise(p * 2.1) * 0.3 + valueNoise(p * 4.3) * 0.2;
        cloud = smoothstep(0.45, 0.75, cloud);
        sky = lerp(sky, float3(0.16, 0.19, 0.30), cloud * 0.85);
    }
    return sky;
}

[shader("compute")]
[numthreads(8, 8, 1)]
void traceView(uint3 id: SV_DispatchThreadID)
{
    View* view = push.view;
    World* world = push.world;
    if (id.x >= view->size.x || id.y >= view->size.y)
    {
        return;
    }
    float2 ndc = (float2(id.xy) + 0.5) / float2(view->size) * 2.0 - 1.0;
    float3 d = normalize(view->forward + view->right * (ndc.x * view->tanHalf * view->aspect) + view->up * (ndc.y * view->tanHalf));
    float footScale = view->pixelAngle * view->lodScale;
    Hit hit;
    float3 colour;
    bool found = trace(world, view->eye, d, 1e9, 0.0, footScale, hit);
    int steps = hit.steps;
    if (found)
    {
        int3 n = int3(0);
        if (hit.axis >= 0)
        {
            n[hit.axis] = d[hit.axis] > 0.0 ? -1 : 1;
        }
        float3 at = view->eye + d * hit.t;
        float3 albedo = colourOf(world, hit);
        // Each voxel a shade of its own, as the reference's small cubes are.
        float grain = float(hashOf(hit.cell >> hit.level) >> 8) / 16777216.0;
        albedo *= 0.85 + 0.3 * grain;
        float3 normal = float3(n);
        float lit = saturate(dot(normal, view->sun));
        float shadow = 1.0;
        if ((view->flags & SHADOWS) != 0u && lit > 0.0)
        {
            Hit blocked;
            float foot = hit.t * footScale;
            // A hit on a coarser level is a whole group's or brick's face: the shadow ray starts a group or brick out,
            // in the cell the view's ray came through, and walks the same level, or it would meet its own cell.
            float out = hit.level == 0 ? 0.02 : (hit.level == 1 ? 2.02 : 8.02);
            float3 from = at + normal * out;
            if (trace(world, from, view->sun, 64.0 / world->voxel, foot, 0.0, blocked))
            {
                shadow = 0.0;
            }
            steps += blocked.steps;
        }
        float open = 1.0;
        if ((view->flags & OCCLUSION) != 0u && hit.level == 0 && hit.axis >= 0)
        {
            open = occlusion(world, hit.cell, hit.axis, n, at);
        }
        float3 moon = float3(0.50, 0.62, 1.0) * 1.5;
        float3 ambient = float3(0.16, 0.21, 0.38) * (0.6 + 0.4 * normal.y) * (0.35 + 0.65 * open);
        colour = albedo * (moon * lit * shadow + ambient);
        float metres = hit.t * world->voxel;
        colour = lerp(colour, float3(0.07, 0.09, 0.18), 1.0 - exp(-metres * 0.0025));
    }
    else
    {
        colour = skyOf(d);
    }
    if ((view->flags & SHOW_STEPS) != 0u)
    {
        float s = float(steps) / 200.0;
        colour = float3(saturate(s), saturate(s - 1.0), saturate(1.0 - s)) * 0.8;
    }
    colour *= 1.3;
    colour = colour / (1.0 + colour);
    colour = pow(colour, float3(1.0 / 2.2));
    uint3 c = uint3(saturate(colour) * 255.0 + 0.5);
    view->output[id.y * view->size.x + id.x] = c.x | (c.y << 8) | (c.z << 16) | 0xFF000000u;
}
