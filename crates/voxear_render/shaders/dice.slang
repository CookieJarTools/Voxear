// Dicing (src/dice.rs): displaced surfaces cut finely enough near the
// camera to show their height maps, coarsely far off. The cull pass lists
// the displaced instances in view; then two compute passes cut each of
// their triangles:
//
// - Split: a thread for each triangle works out how many pixels long each
//   edge is, and cuts the triangle into M×M smaller ones, as many as keep
//   each piece's edges within DICE_MOST pieces.
// - Patches: a workgroup for each split triangle sends each of its pieces
//   in view out as a draw of its own, an N×N grid of triangles.
//
// The vertex shader finds its piece and its place in the piece's grid from
// the draw and vertex, and moves it out by the height map. Each edge is
// diced as finely as its own length on screen asks, worked out from its two
// ends, which the triangles either side of it share: a vertex on an edge
// snaps to one of its edge's even steps, so both sides make the same points
// and no crack opens between them.

#include "culling.slang"
#include "shading.slang"

// How many pieces a split triangle's edge is cut into at most, and how many
// times each piece's edges are cut.
static const uint DICE_SPLIT = 32;
static const uint DICE_MOST = 64;
// The most displaced instances, split triangles and pieces a frame (dice::MAX_*).
static const uint DICE_MAX_INSTANCES = 4096;
static const uint DICE_MAX_SPLITS = 65535;
static const uint DICE_MAX_PATCHES = 262144;
// Where the counts are in Buffers::diceCounts: the split pass's dispatch, the
// patch pass's, how many patches, and how many did not fit.
static const uint DICE_INSTANCES = 0;
static const uint DICE_SPLITS = 4;
static const uint DICE_PATCHES = 8;
static const uint DICE_OVERFLOW = 9;

// How many times an edge from `a` to `b`, in the world, is cut: as many as
// it spans pixels over `detail`, at most `most`.
float diceFactor(Frame* frame, float3 a, float3 b, float detail, float most)
{
    float away = max(length((a + b) * 0.5 - frame->cameraPosition), frame->near);
    float pixels = length(b - a) * frame->lodScale / away;
    return clamp(ceil(pixels / detail), 1.0, most);
}

// One of an instance's LOD0 triangles: its three vertices' places in the
// vertex buffer.
uint3 diceTriangle(Mesh mesh, uint triangle)
{
    uint first = mesh.lods[0].indexOffset + triangle * 3;
    return uint3(buffers()->indices[first], buffers()->indices[first + 1], buffers()->indices[first + 2]) + mesh.vertexOffset;
}

// The edge factor of the edge from vertex `i` to vertex `j`, the same
// whichever triangle asks: from its lower-numbered end.
float diceEdge(Frame* frame, Instance instance, uint i, uint j, float detail, float most)
{
    uint low = min(i, j);
    uint high = max(i, j);
    float3 a = transform(instance.model, float4(buffers()->vertices[low].position, 1.0)).xyz;
    float3 b = transform(instance.model, float4(buffers()->vertices[high].position, 1.0)).xyz;
    return diceFactor(frame, a, b, detail, most);
}

// Splits each triangle of a displaced instance in view (a workgroup each).
[shader("compute")]
[numthreads(64, 1, 1)]
void diceSplitMain(uint3 group : SV_GroupID, uint3 thread : SV_GroupThreadID)
{
    Frame* frame = push.frame;
    if (group.x >= DICE_MAX_INSTANCES)
    {
        return;
    }
    uint index = buffers()->dicedInstances[group.x];
    Instance instance = buffers()->instances[index];
    Mesh mesh = buffers()->meshes[instance.mesh];
    Material material = buffers()->materials[instance.material];
    float margin = abs(material.displacement) * 0.5;
    uint triangles = mesh.lods[0].indexCount / 3;
    for (uint t = thread.x; t < triangles; t += 64)
    {
        uint3 v = diceTriangle(mesh, t);
        float3 p0 = transform(instance.model, float4(buffers()->vertices[v.x].position, 1.0)).xyz;
        float3 p1 = transform(instance.model, float4(buffers()->vertices[v.y].position, 1.0)).xyz;
        float3 p2 = transform(instance.model, float4(buffers()->vertices[v.z].position, 1.0)).xyz;
        float3 middle = (p0 + p1 + p2) / 3.0;
        float reach = sqrt(max(max(dot(p0 - middle, p0 - middle), dot(p1 - middle, p1 - middle)), dot(p2 - middle, p2 - middle)));
        if (!insidePlanes(frame->planes, middle, reach + margin))
        {
            continue;
        }
        // Facing away by more than its displacement could turn it, seen from the side its normals face.
        float3 facing = cross(p1 - p0, p2 - p0);
        float3 normals = buffers()->vertices[v.x].normal + buffers()->vertices[v.y].normal + buffers()->vertices[v.z].normal;
        float3 worldNormals = instance.normal0.xyz * normals.x + instance.normal1.xyz * normals.y + instance.normal2.xyz * normals.z;
        facing = normalizeOr(dot(facing, worldNormals) < 0.0 ? -facing : facing, float3(0.0, 1.0, 0.0));
        if (dot(facing, frame->cameraPosition - p0) < -margin)
        {
            continue;
        }
        float most = float(DICE_SPLIT * DICE_MOST);
        float f0 = diceEdge(frame, instance, v.x, v.y, material.detail, most);
        float f1 = diceEdge(frame, instance, v.y, v.z, material.detail, most);
        float f2 = diceEdge(frame, instance, v.z, v.x, material.detail, most);
        uint split = clamp(uint(ceil(max(f0, max(f1, f2)) / float(DICE_MOST))), 1u, DICE_SPLIT);
        uint at;
        InterlockedAdd(buffers()->diceCounts[DICE_SPLITS], 1, at);
        if (at >= DICE_MAX_SPLITS)
        {
            InterlockedAdd(buffers()->diceCounts[DICE_OVERFLOW], 1);
            continue;
        }
        buffers()->diceSplits[at] = uint4(index, t, split | (uint(f0) << 16), uint(f1) | (uint(f2) << 16));
    }
}

// A split triangle's piece `piece`, as its three corners on the split
// triangle's grid of `split` steps a side: (i, j) as the share of the way
// to the second and third vertices, times `split`.
void dicePiece(uint split, uint piece, out int2 a, out int2 b, out int2 c)
{
    // Rows from the top: row q from the top has 2q + 1 pieces, q² before it.
    uint q = uint(floor(sqrt(float(piece))));
    q = (q + 1) * (q + 1) <= piece ? q + 1 : (q * q > piece ? q - 1 : q);
    uint k = piece - q * q;
    int j = int(split - 1 - q);
    int i = int(k / 2);
    if ((k & 1) == 0)
    {
        a = int2(i, j);
        b = int2(i + 1, j);
        c = int2(i, j + 1);
        return;
    }
    a = int2(i + 1, j);
    b = int2(i + 1, j + 1);
    c = int2(i, j + 1);
}

// Where a point on the split grid is in the instance's mesh space.
float3 diceGridPoint(float3 p0, float3 p1, float3 p2, float2 g, uint split)
{
    float2 b = g / float(split);
    return p0 * (1.0 - b.x - b.y) + p1 * b.x + p2 * b.y;
}

// Which of the triangle's edges a piece's edge from `a` to `b` lies on, 0
// for v0–v1, 1 for v1–v2, 2 for v2–v0, or 3 for none.
uint diceOnEdge(int2 a, int2 b, uint split)
{
    if (a.y == 0 && b.y == 0)
    {
        return 0;
    }
    if (a.x + a.y == int(split) && b.x + b.y == int(split))
    {
        return 1;
    }
    return a.x == 0 && b.x == 0 ? 2 : 3;
}

// How many times a piece's edge from `a` to `b` is cut: on the triangle's
// own edge, its share of that edge's factor; inside it, from its own ends.
float dicePieceEdge(Frame* frame, Instance instance, float3 corners[3], int2 a, int2 b, uint split, uint3 factors, float detail)
{
    uint edge = diceOnEdge(a, b, split);
    if (edge < 3)
    {
        return min(ceil(float(factors[edge]) / float(split)), float(DICE_MOST));
    }
    // From the lower end on the grid, so the pieces either side work it out alike.
    bool flip = a.y > b.y || (a.y == b.y && a.x > b.x);
    int2 low = flip ? b : a;
    int2 high = flip ? a : b;
    float3 from = transform(instance.model, float4(diceGridPoint(corners[0], corners[1], corners[2], float2(low), split), 1.0)).xyz;
    float3 to = transform(instance.model, float4(diceGridPoint(corners[0], corners[1], corners[2], float2(high), split), 1.0)).xyz;
    return diceFactor(frame, from, to, detail, float(DICE_MOST));
}

// Sends out each piece in view of a split triangle as a draw (a workgroup
// each).
[shader("compute")]
[numthreads(64, 1, 1)]
void dicePatchMain(uint3 group : SV_GroupID, uint3 thread : SV_GroupThreadID)
{
    Frame* frame = push.frame;
    uint splitIndex = group.x;
    if (splitIndex >= min(buffers()->diceCounts[DICE_SPLITS], DICE_MAX_SPLITS))
    {
        return;
    }
    uint4 record = buffers()->diceSplits[splitIndex];
    Instance instance = buffers()->instances[record.x];
    Mesh mesh = buffers()->meshes[instance.mesh];
    Material material = buffers()->materials[instance.material];
    uint split = record.z & 0xFFFF;
    uint3 factors = uint3(record.z >> 16, record.w & 0xFFFF, record.w >> 16);
    uint3 v = diceTriangle(mesh, record.y);
    float3 corners[3] = { buffers()->vertices[v.x].position, buffers()->vertices[v.y].position, buffers()->vertices[v.z].position };
    float margin = abs(material.displacement) * 0.5;
    for (uint piece = thread.x; piece < split * split; piece += 64)
    {
        int2 a;
        int2 b;
        int2 c;
        dicePiece(split, piece, a, b, c);
        float3 pa = transform(instance.model, float4(diceGridPoint(corners[0], corners[1], corners[2], float2(a), split), 1.0)).xyz;
        float3 pb = transform(instance.model, float4(diceGridPoint(corners[0], corners[1], corners[2], float2(b), split), 1.0)).xyz;
        float3 pc = transform(instance.model, float4(diceGridPoint(corners[0], corners[1], corners[2], float2(c), split), 1.0)).xyz;
        float3 middle = (pa + pb + pc) / 3.0;
        float reach = sqrt(max(max(dot(pa - middle, pa - middle), dot(pb - middle, pb - middle)), dot(pc - middle, pc - middle)));
        if (!insidePlanes(frame->planes, middle, reach + margin))
        {
            continue;
        }
        uint fab = uint(dicePieceEdge(frame, instance, corners, a, b, split, factors, material.detail));
        uint fbc = uint(dicePieceEdge(frame, instance, corners, b, c, split, factors, material.detail));
        uint fca = uint(dicePieceEdge(frame, instance, corners, c, a, split, factors, material.detail));
        uint n = max(fab, max(fbc, fca));
        uint at;
        InterlockedAdd(buffers()->diceCounts[DICE_PATCHES], 1, at);
        if (at >= DICE_MAX_PATCHES)
        {
            InterlockedAdd(buffers()->diceCounts[DICE_OVERFLOW], 1);
            InterlockedAdd(buffers()->stats[STAT_DICE_OVERFLOW], 1);
            continue;
        }
        InterlockedAdd(buffers()->stats[STAT_DICED_PATCHES], 1);
        buffers()->dicePatches[at] = uint4(splitIndex, piece, n | (fab << 8) | (fbc << 16) | (fca << 24), 0);
        buffers()->diceDraws[at] = uint4(n * n * 3, 1, 0, 0);
    }
}

// How far along the triangle's own edge `on` grid point `g` lies, from the
// edge's lower-numbered vertex, snapped to one of the edge's `factors[on]`
// even steps.
float diceAlong(uint on, float2 g, uint split, uint3 factors, uint3 v)
{
    float share = on == 0 ? g.x / float(split) : (on == 1 ? g.y / float(split) : 1.0 - g.y / float(split));
    uint first = on == 0 ? v.x : (on == 1 ? v.y : v.z);
    uint second = on == 0 ? v.y : (on == 1 ? v.z : v.x);
    share = first > second ? 1.0 - share : share;
    float whole = float(factors[on]);
    return round(share * whole) / whole;
}

// Which of the triangle's edges grid point `p` lies on, or 3 for none.
uint diceCornerOn(int2 p, uint split)
{
    if (p.y == 0)
    {
        return 0;
    }
    if (p.x + p.y == int(split))
    {
        return 1;
    }
    return p.x == 0 ? 2 : 3;
}

// Where on the split triangle's grid a point of a piece's own grid lies:
// (u, w) steps of `n` from corner `a` towards `b` and `c`. A point on one of
// the piece's edges snaps to that edge's steps, counted from the edge's
// lower end. A point on the triangle's own edge gives that edge in `edge`,
// and how far along it from its lower-numbered vertex in `along`, snapped
// to the whole edge's steps; otherwise `edge` is 3.
float2 diceSnap(int2 a, int2 b, int2 c, uint u, uint w, uint n, uint3 cuts, uint split, uint3 factors, uint3 v, out uint edge, out float along)
{
    edge = 3;
    along = 0.0;
    float2 fa = float2(a);
    float2 g = fa + (float2(b) - fa) * (float(u) / float(n)) + (float2(c) - fa) * (float(w) / float(n));
    // A corner of the piece, on the grid exactly.
    bool cornered = (u == 0 || u == n) && (w == 0 || w == n) && u + w <= n;
    if (cornered)
    {
        int2 corner = u == n ? b : (w == n ? c : a);
        g = float2(corner);
        edge = diceCornerOn(corner, split);
        along = edge < 3 ? diceAlong(edge, g, split, factors, v) : 0.0;
        return g;
    }
    // Which of the piece's edges it is on, if any: its ends, how far from the first, and how many times it is cut.
    int2 from = a;
    int2 to = b;
    float t = float(u) / float(n);
    float cut = float(cuts.x);
    if (u + w == n)
    {
        from = b;
        to = c;
        t = float(w) / float(n);
        cut = float(cuts.y);
    }
    else if (u == 0)
    {
        from = a;
        to = c;
        t = float(w) / float(n);
        cut = float(cuts.z);
    }
    else if (w != 0)
    {
        return g;
    }
    uint on = diceOnEdge(from, to, split);
    if (on < 3)
    {
        g = float2(from) + float2(to - from) * t;
        edge = on;
        along = diceAlong(on, g, split, factors, v);
        return g;
    }
    // Inside the triangle, from the edge's lower end on the grid.
    bool flip = from.y > to.y || (from.y == to.y && from.x > to.x);
    float2 low = float2(flip ? to : from);
    float2 high = float2(flip ? from : to);
    float s = round((flip ? 1.0 - t : t) * cut) / cut;
    return low + (high - low) * s;
}

// Two vertices `share` of the way from `a` to `b`.
Vertex diceMix(Vertex a, Vertex b, float share)
{
    Vertex o;
    o.position = lerp(a.position, b.position, share);
    o.normal = lerp(a.normal, b.normal, share);
    o.uv = lerp(a.uv, b.uv, share);
    o.tangent = lerp(a.tangent, b.tangent, share);
    o.wind = a.wind;
    return o;
}

// The height map's height at `uv`, -0.5 to 0.5, at mip `level`.
float diceHeight(Material material, float2 uv, float level)
{
    return textures[NonUniformResourceIndex(material.heightTexture)].SampleLevel(samplers[SAMPLER_LINEAR_REPEAT], uv, level).r - 0.5;
}

// A vertex of a piece: its draw finds its piece, the vertex its place in
// the piece's grid, then it is moved out along its normal by the height
// map, now and through last frame's matrix for its motion.
VertexOut dicedPoint(uint vertexId, uint drawId)
{
    Frame* frame = push.frame;
    uint4 patch = buffers()->dicePatches[drawId];
    uint4 record = buffers()->diceSplits[patch.x];
    uint instanceIndex = record.x;
    Instance instance = buffers()->instances[instanceIndex];
    Mesh mesh = buffers()->meshes[instance.mesh];
    Material material = buffers()->materials[instance.material];
    uint split = record.z & 0xFFFF;
    uint3 factors = uint3(record.z >> 16, record.w & 0xFFFF, record.w >> 16);
    uint n = patch.z & 0xFF;
    uint3 cuts = uint3((patch.z >> 8) & 0xFF, (patch.z >> 16) & 0xFF, patch.z >> 24);
    uint3 v = diceTriangle(mesh, record.y);
    int2 a;
    int2 b;
    int2 c;
    dicePiece(split, patch.y, a, b, c);
    // This vertex's place in the piece's own grid, as dicePiece places pieces in the triangle.
    int2 pa;
    int2 pb;
    int2 pc;
    dicePiece(n, vertexId / 3, pa, pb, pc);
    uint corner = vertexId % 3;
    int2 at = corner == 0 ? pa : (corner == 1 ? pb : pc);
    uint edge;
    float along;
    float2 g = diceSnap(a, b, c, uint(at.x), uint(at.y), n, cuts, split, factors, v, edge, along);

    Vertex v0 = buffers()->vertices[v.x];
    Vertex v1 = buffers()->vertices[v.y];
    Vertex v2 = buffers()->vertices[v.z];
    Vertex here = v0;
    if (edge < 3)
    {
        // On the triangle's own edge, mixed from its two ends alone, lower-numbered first, so both sides agree.
        uint first = edge == 0 ? v.x : (edge == 1 ? v.y : v.z);
        uint second = edge == 0 ? v.y : (edge == 1 ? v.z : v.x);
        here = diceMix(buffers()->vertices[min(first, second)], buffers()->vertices[max(first, second)], along);
    }
    else
    {
        float2 bary = g / float(split);
        here.position = v0.position * (1.0 - bary.x - bary.y) + v1.position * bary.x + v2.position * bary.y;
        here.normal = v0.normal * (1.0 - bary.x - bary.y) + v1.normal * bary.x + v2.normal * bary.y;
        here.uv = v0.uv * (1.0 - bary.x - bary.y) + v1.uv * bary.x + v2.uv * bary.y;
        here.tangent = v0.tangent * (1.0 - bary.x - bary.y) + v1.tangent * bary.x + v2.tangent * bary.y;
    }

    // The height map read as finely as the camera sees it here: far off, from smaller mips, so it does not shimmer.
    float3 world = transform(instance.model, float4(here.position, 1.0)).xyz;
    float away = max(length(world - frame->cameraPosition), frame->near);
    uint width;
    uint height;
    textures[NonUniformResourceIndex(material.heightTexture)].GetDimensions(width, height);
    float metresAPixel = away / frame->lodScale;
    float level = max(log2(max(metresAPixel * material.detail * float(width) * 0.5, 1.0)), 0.0);
    float rise = diceHeight(material, here.uv, level);

    // Bent by the height's slope along the triangle's surface, as far as its UVs stretch over it.
    float3 normal = normalizeOr(here.normal, float3(0.0, 1.0, 0.0));
    float3 tangent = normalizeOr(here.tangent.xyz - normal * dot(here.tangent.xyz, normal), float3(1.0, 0.0, 0.0));
    float3 bitangent = cross(normal, tangent) * (here.tangent.w < 0.0 ? -1.0 : 1.0);
    float2 duv1 = v1.uv - v0.uv;
    float2 duv2 = v2.uv - v0.uv;
    float3 dp1 = v1.position - v0.position;
    float3 dp2 = v2.position - v0.position;
    float area = duv1.x * duv2.y - duv1.y * duv2.x;
    float metresAUv = abs(area) > 1e-10 ? sqrt(length(cross(dp1, dp2)) / abs(area)) : 1.0;
    float step = exp2(level) / float(max(width, 1u));
    float slopeU = (diceHeight(material, here.uv + float2(step, 0.0), level) - diceHeight(material, here.uv - float2(step, 0.0), level)) / (2.0 * step * metresAUv);
    float slopeV = (diceHeight(material, here.uv + float2(0.0, step), level) - diceHeight(material, here.uv - float2(0.0, step), level)) / (2.0 * step * metresAUv);
    // Displacement is in metres in the world: in the mesh's own space, as much shorter as the Obj is scaled up along it.
    float metresAUnit = max(length(transform(instance.model, float4(normal, 0.0)).xyz), 1e-6);
    float reach = material.displacement / metresAUnit;
    float3 bent = normalizeOr(normal - (tangent * slopeU + bitangent * slopeV) * reach, normal);

    float3 moved = here.position + normal * rise * reach;
    float4 now = transform(instance.model, float4(moved, 1.0));
    float4 before = transform(instance.prevModel, float4(moved, 1.0));

    VertexOut o;
    o.position = transform(frame->viewProj, now);
    o.worldPosition = now.xyz;
    o.normal = instance.normal0.xyz * bent.x + instance.normal1.xyz * bent.y + instance.normal2.xyz * bent.z;
    o.tangent = float4(transform(instance.model, float4(tangent, 0.0)).xyz, here.tangent.w);
    o.uv = here.uv;
    o.material = instance.material;
    o.lod = 0;
    o.instance = instanceIndex;
    o.fade = 0;
    o.cluster = 0;
    o.currentClip = transform(frame->unjitteredViewProj, now);
    o.previousClip = transform(frame->prevViewProj, before);
    o.moved = 0.0;
    return o;
}

[shader("vertex")]
VertexOut dicedVertex(uint vertexId : SV_VertexID, uint drawId : SV_DrawIndex)
{
    return dicedPoint(vertexId, drawId);
}

// A vertex of a piece in a sun cascade's view (push.list), for the pieces'
// own shadows: the flat mesh they were diced from casts none.
[shader("vertex")]
float4 dicedShadowVertex(uint vertexId : SV_VertexID, uint drawId : SV_DrawIndex) : SV_Position
{
    VertexOut o = dicedPoint(vertexId, drawId);
    return transform(shadowViewProj(push.frame, push.list, push.level), float4(o.worldPosition, 1.0));
}
