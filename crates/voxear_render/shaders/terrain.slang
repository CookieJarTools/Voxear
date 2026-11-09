// Terrain (src/terrain.rs): each patch a grid of TERRAIN_PATCH_QUADS quads
// each way, its vertices placed here from their index in the grid (one
// index buffer, the same for every patch, joins them into triangles), its
// heights read from the terrain's tiles. A vertex slides onto the next level's grid, every other
// one onto its neighbour, as it nears its level's far distance, so a level
// meets the next with no crack. The quads are split from their lowest
// corner to their highest, as the CPU's ground and physics split them.
//
// The ground is shaded by shading.slang's fragment shaders, as any mesh
// is: the terrain's anchor instance stands in for an instance's own.

#include "shading.slang"

// The height of sample `sample` of the whole terrain, clamped to its edges.
float terrainSample(Terrain terrain, int2 sample)
{
    int2 quads = int2(terrain.tiles * TERRAIN_TILE_QUADS);
    sample = clamp(sample, int2(0), quads);
    int2 tile = min(sample / int(TERRAIN_TILE_QUADS), int2(terrain.tiles) - 1);
    int2 inside = sample - tile * int(TERRAIN_TILE_QUADS);
    uint slot = uint(tile.y) * terrain.tiles.x + uint(tile.x);
    return terrain.heights[slot * TERRAIN_TILE_LEN + uint(inside.y) * TERRAIN_TILE_SAMPLES + uint(inside.x)];
}

// The ground's height at `local`, metres from the terrain's corner, on the triangles between samples.
float terrainHeight(Terrain terrain, float2 local)
{
    float2 quads = float2(terrain.tiles * TERRAIN_TILE_QUADS);
    float2 f = clamp(local / terrain.spacing, float2(0.0), quads);
    int2 i = min(int2(floor(f)), int2(quads) - 1);
    float2 t = f - float2(i);
    float h00 = terrainSample(terrain, i);
    float h10 = terrainSample(terrain, i + int2(1, 0));
    float h01 = terrainSample(terrain, i + int2(0, 1));
    float h11 = terrainSample(terrain, i + int2(1, 1));
    return t.y > t.x ? h00 + t.y * (h01 - h00) + t.x * (h11 - h01) : h00 + t.x * (h10 - h00) + t.y * (h11 - h10);
}

struct TerrainPoint
{
    float3 world;
    float3 normal;
    float2 local;
    uint level;
    uint terrain;
};

// Vertex `vertexId` of patch `instanceId` in draw list `list`: where it is, slid onto the next level's grid as
// far as its distance from the camera says, and which way the ground faces there.
TerrainPoint terrainPoint(Frame* frame, uint list, uint vertexId, uint instanceId)
{
    uint first = buffers()->terrainLists[list];
    TerrainPatch patch = buffers()->terrainPatches[first + instanceId];
    uint level = patch.packed & 0xFF;
    Terrain terrain = buffers()->terrains[patch.packed >> 8];

    // The patch's grid has a vertex more than quads each way.
    uint across = TERRAIN_PATCH_QUADS + 1;
    uint2 grid = uint2(vertexId % across, vertexId / across);
    float step = patch.size / float(TERRAIN_PATCH_QUADS);
    float2 local = patch.origin + float2(grid) * step;
    float2 size = float2(terrain.tiles * TERRAIN_TILE_QUADS) * terrain.spacing;

    // Slid by its distance before sliding, every other vertex onto the one before it.
    float3 before = terrain.corner + float3(local.x, terrainHeight(terrain, min(local, size)), local.y);
    float2 range = terrain.morph[min(level, TERRAIN_MAX_LEVELS - 1)];
    float slide = range.y > range.x ? saturate((distance(before, frame->cameraPosition) - range.x) / (range.y - range.x)) : 0.0;
    float2 odd = float2(grid % 2);
    local -= odd * step * slide;
    // A patch reaching past the terrain's far edge folds onto it.
    local = min(local, size);

    TerrainPoint p;
    p.local = local;
    p.world = terrain.corner + float3(local.x, terrainHeight(terrain, local), local.y);
    // Across as wide as this level's quads, so far ground is smoothed as it is drawn.
    float width = max(step, terrain.spacing);
    float dx = terrainHeight(terrain, local + float2(width, 0.0)) - terrainHeight(terrain, local - float2(width, 0.0));
    float dz = terrainHeight(terrain, local + float2(0.0, width)) - terrainHeight(terrain, local - float2(0.0, width));
    p.normal = normalize(float3(-dx, 2.0 * width, -dz));
    p.level = level;
    p.terrain = patch.packed >> 8;
    return p;
}

[shader("vertex")]
VertexOut terrainVertex(uint vertexId : SV_VertexID, uint instanceId : SV_InstanceID)
{
    Frame* frame = push.frame;
    TerrainPoint p = terrainPoint(frame, push.list, vertexId, instanceId);
    Terrain terrain = buffers()->terrains[p.terrain];
    float4 world = float4(p.world, 1.0);

    VertexOut o;
    o.position = transform(frame->viewProj, world);
    o.worldPosition = p.world;
    o.normal = p.normal;
    // Along +x over the ground, as the pictures' u runs; v runs along -z, as a plane's does.
    o.tangent = float4(normalizeOr(float3(p.normal.y, -p.normal.x, 0.0), float3(1.0, 0.0, 0.0)), 1.0);
    o.uv = float2(p.local.x, -p.local.y) * terrain.uvScale;
    o.material = terrain.material;
    o.lod = p.level;
    o.instance = terrain.anchor;
    o.fade = 0;
    o.cluster = 0;
    o.currentClip = transform(frame->unjitteredViewProj, world);
    o.previousClip = transform(frame->prevViewProj, world);
    o.moved = 0.0;
    o.hidden = 0.0;
    return o;
}

// A vertex in a sun cascade's view (push.list), placed as the camera's view places it, so the ground's shadow
// falls from the very ground drawn.
[shader("vertex")]
float4 terrainShadowVertex(uint vertexId : SV_VertexID, uint instanceId : SV_InstanceID) : SV_Position
{
    TerrainPoint p = terrainPoint(push.frame, push.list, vertexId, instanceId);
    return transform(shadowViewProj(push.frame, push.list, 0), float4(p.world, 1.0));
}
