// Small voxels drawn brick by brick: each brick's box is drawn with the GPU's triangles, and each pixel of it walks
// its ray inside that brick alone, at most two dozen steps. The depth test throws away what lies behind nearer
// bricks, so bricks are put in order first, nearest band of distance first: a compute pass counts each band's
// bricks in view, one works out where each band starts, and one writes each brick into its band.

#include "common.slang"

uint3 placeOf(uint packed)
{
    return uint3(packed & 2047u, (packed >> 11) & 1023u, packed >> 21);
}

// The band of distance a brick in view falls in, or BANDS for one out of view.
uint bandOf(View* view, uint brick)
{
    float3 middle = float3(placeOf(push.draw->places[brick]) * 8u) + 4.0 - view->eye;
    float z = dot(middle, view->forward);
    float x = dot(middle, view->right);
    float y = dot(middle, view->up);
    float radius = 6.93;
    if (z < -radius)
    {
        return BANDS;
    }
    float wide = view->tanHalf * view->aspect;
    float high = view->tanHalf;
    if (abs(x) - z * wide > radius * sqrt(1.0 + wide * wide) || abs(y) - z * high > radius * sqrt(1.0 + high * high))
    {
        return BANDS;
    }
    return min(uint(log2(max(z, 8.0) / 8.0) * 3.0), BANDS - 1);
}

[shader("compute")]
[numthreads(256, 1, 1)]
void countBands(uint3 id: SV_DispatchThreadID)
{
    Draw* draw = push.draw;
    if (id.x >= draw->brickCount)
    {
        return;
    }
    uint band = bandOf(push.view, id.x);
    if (band < BANDS)
    {
        InterlockedAdd(draw->bands[band], 1u);
    }
}

[shader("compute")]
[numthreads(1, 1, 1)]
void startBands(uint3 id: SV_DispatchThreadID)
{
    Draw* draw = push.draw;
    uint total = 0;
    for (uint band = 0; band < BANDS; band++)
    {
        draw->bands[BANDS + band] = total;
        total += draw->bands[band];
    }
    // vertexCount, instanceCount, firstVertex, firstInstance
    draw->draws[0] = 18;
    draw->draws[1] = total;
    draw->draws[2] = 0;
    draw->draws[3] = 0;
}

[shader("compute")]
[numthreads(256, 1, 1)]
void sortBands(uint3 id: SV_DispatchThreadID)
{
    Draw* draw = push.draw;
    if (id.x >= draw->brickCount)
    {
        return;
    }
    uint band = bandOf(push.view, id.x);
    if (band < BANDS)
    {
        uint slot;
        InterlockedAdd(draw->bands[2 * BANDS + band], 1u, slot);
        draw->drawn[draw->bands[BANDS + band] + slot] = id.x;
    }
}

struct BrickVertex
{
    float4 position : SV_Position;
    float3 at : POSITION;
    nointerpolation uint brick : BRICK;
};

// Three sides of a brick's box at most face the camera: two triangles each, or none where the camera is level
// with that pair of sides.
[shader("vertex")]
BrickVertex brickVertex(uint vertex: SV_VertexID, uint instance: SV_InstanceID)
{
    View* view = push.view;
    Draw* draw = push.draw;
    uint brick = draw->drawn[instance];
    float3 low = float3(placeOf(draw->places[brick]) * 8u);
    float3 high = low + 8.0;
    uint axis = vertex / 6;
    uint corner = vertex % 6;
    static const float2 QUAD[6] = { float2(0, 0), float2(1, 0), float2(1, 1), float2(0, 0), float2(1, 1), float2(0, 1) };
    float3 p;
    bool shown = true;
    if (view->eye[axis] < low[axis])
    {
        p[axis] = low[axis];
    }
    else if (view->eye[axis] > high[axis])
    {
        p[axis] = high[axis];
    }
    else
    {
        p[axis] = low[axis];
        shown = false;
    }
    uint u = (axis + 1) % 3;
    uint v = (axis + 2) % 3;
    p[u] = lerp(low[u], high[u], QUAD[corner].x);
    p[v] = lerp(low[v], high[v], QUAD[corner].y);
    float3 r = p - view->eye;
    float z = dot(r, view->forward);
    BrickVertex out;
    out.position = shown ? float4(dot(r, view->right) / (view->tanHalf * view->aspect), dot(r, view->up) / view->tanHalf, z - draw->near, z) : float4(0.0, 0.0, 0.0, 0.0) / 0.0;
    out.at = p;
    out.brick = brick;
    return out;
}

struct BrickPixel
{
    uint2 seen : SV_Target0;
    float depth : SV_DepthGreaterEqual;
};

bool brickBit(uint* bits, int3 l, int size)
{
    if (size == 1)
    {
        return bitOf(bits, uint(l.x + 8 * (l.y + 8 * l.z)));
    }
    int3 g = l >> 1;
    return bitOf(bits + 16, uint(g.x + 4 * (g.y + 4 * g.z)));
}

[shader("fragment")]
BrickPixel brickFragment(BrickVertex input)
{
    View* view = push.view;
    World* world = push.world;
    Draw* draw = push.draw;
    float3 o = view->eye;
    float3 d = normalize(input.at - o);
    d = select(abs(d) < 1e-7, select(d < 0.0, -1e-7, 1e-7), d);
    float3 inv = 1.0 / d;
    int3 low = int3(placeOf(draw->places[input.brick]) * 8u);
    float3 t0 = (float3(low) - o) * inv;
    float3 t1 = (float3(low) + 8.0 - o) * inv;
    float3 tn = min(t0, t1);
    float3 tf = max(t0, t1);
    float tEnter = max(max(tn.x, tn.y), max(tn.z, 0.0));
    float tExit = min(min(tf.x, tf.y), tf.z);
    if (tEnter >= tExit)
    {
        discard;
    }
    int axis = tn.x >= tn.y && tn.x >= tn.z ? 0 : (tn.y >= tn.z ? 1 : 2);
    float foot = tEnter * view->pixelAngle * view->lodScale;
    uint* bits = world->bricks + input.brick * BRICK_WORDS;
    int3 up = int3(d > 0.0);
    int3 c = clamp(int3(floor(o + d * tEnter)), low, low + 7);
    c[axis] = d[axis] > 0.0 ? low[axis] : low[axis] + 7;
    Hit hit = (Hit)0;
    hit.brick = int(input.brick);
    hit.level = foot >= 8.0 ? 2 : (foot >= 2.0 ? 1 : 0);
    float t = tEnter;
    bool found = hit.level == 2;
    int size = hit.level == 1 ? 2 : 1;
    int steps = 0;
    while (!found && steps < 26)
    {
        steps++;
        if (brickBit(bits, c - low, size))
        {
            found = true;
            break;
        }
        int3 corner = c & ~(size - 1);
        float3 te = (float3(corner + up * size) - o) * inv;
        axis = te.x < te.y && te.x < te.z ? 0 : (te.y < te.z ? 1 : 2);
        t = te[axis];
        if (t >= tExit)
        {
            break;
        }
        int edge = corner[axis] + up[axis] * size;
        c = clamp(int3(floor(o + d * t)), low, low + 7);
        c[axis] = d[axis] > 0.0 ? edge : edge - 1;
    }
    if (!found)
    {
        discard;
    }
    // Only what was seen is written: it is shaded once a pixel afterwards (shadeBricks), not for every brick drawn
    // over another.
    int3 l = c - low;
    BrickPixel pixel;
    pixel.seen = uint2(input.brick, uint(l.x + 8 * (l.y + 8 * l.z)) | (uint(axis) << 9) | (hit.level << 11) | (uint(steps) << 16));
    float z = dot(d * t, view->forward);
    pixel.depth = (z - draw->near) / z;
    return pixel;
}

[shader("compute")]
[numthreads(8, 8, 1)]
void shadeBricks(uint3 id: SV_DispatchThreadID)
{
    View* view = push.view;
    World* world = push.world;
    Draw* draw = push.draw;
    if (id.x >= view->size.x || id.y >= view->size.y)
    {
        return;
    }
    uint at = id.y * view->size.x + id.x;
    float3 d = rayOf(view, float2(id.xy) + 0.5);
    uint2 seen = draw->seen[at];
    if (seen.x == EMPTY)
    {
        view->output[at] = finish(skyOf(d), view->flags, 0);
        return;
    }
    Hit hit;
    hit.brick = int(seen.x);
    uint index = seen.y & 511u;
    hit.cell = int3(placeOf(draw->places[seen.x]) * 8u) + int3(index & 7u, (index >> 3) & 7u, index >> 6);
    hit.axis = int((seen.y >> 9) & 3u);
    hit.level = (seen.y >> 11) & 3u;
    int steps = int(seen.y >> 16);
    hit.steps = steps;
    // The distance back from the depth: depth = (z - near) / z along the view's forward axis.
    float z = draw->near / (1.0 - draw->depths[at]);
    hit.t = z / dot(d, view->forward);
    view->output[at] = finish(shadeHit(world, view, d, hit, steps), view->flags, steps);
}
