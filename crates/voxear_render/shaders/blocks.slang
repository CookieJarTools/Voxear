// Worlds of blocks (src/blocks.rs): each quad of a chunk's faces two words (voxear_terrain::blocks::mesh), six
// vertices made of it here from their number, as no vertex buffer holds them: where it starts in its chunk, how many
// blocks it spans across and up its face, which way it faces, its surface, how dark each corner is, and its chunk's
// number in the world's pool, which finds where the chunk is. A quad is split along the diagonal between its lighter
// corners, so its shading runs smoothly across it.
//
// Its faces are shaded by shading.slang's fragment shaders, as the terrain's are: each face with its surface's
// material, its pictures repeated each block, darker at the corners blocks close in.
//
// A world drawn smooth (voxear_terrain::blocks::smooth) holds triangles' corners in place of quads, two pairs of words
// each, a vertex each: where it is in its chunk and the chunk's number, then the way the ground faces there and the
// surfaces of its block of ground for its top, sides and bottom, which its fragments blend by the way they face.

#define TERRAIN_SHADING
#include "shading.slang"

// For each way a face faces (+x, -x, +y, -y, +z, -z): the axis it faces along, and the two across it, the first
// crossed with the second facing out, as voxear_terrain::blocks::mesh has them.
static const uint3 BLOCK_AXES[6] = {
    uint3(0, 1, 2), uint3(0, 2, 1), uint3(1, 2, 0), uint3(1, 0, 2), uint3(2, 0, 1), uint3(2, 1, 0)
};

// A quad's corners in the order its triangles take them: split from its first corner to its third, or from its
// second to its fourth.
static const uint BLOCK_CORNERS[6] = { 0, 1, 2, 0, 2, 3 };
static const uint BLOCK_FLIPPED[6] = { 1, 2, 3, 1, 3, 0 };

float3 blockAxis(uint axis)
{
    return float3(axis == 0 ? 1.0 : 0.0, axis == 1 ? 1.0 : 0.0, axis == 2 ? 1.0 : 0.0);
}

struct BlockPoint
{
    float3 world;
    float3 normal;
    uint surface;
    // How light the corner is, 0 closed in to 1 open.
    float shade;
};

// Vertex `vertexId` of a world drawn smooth: its corners one after another in the world's pool from `push.output` on,
// two pairs of words each. Its surface is its block's top, sides and bottom, a byte each.
BlockPoint smoothPoint(BlockWorld world, uint vertexId)
{
    uint at = push.output + vertexId * 2;
    uint2 first = world.quads[at];
    uint2 second = world.quads[at + 1];
    // In 1024ths of a block from two blocks before its chunk's corner.
    float3 local = float3(first.x & 0xFFFF, first.x >> 16, first.y & 0xFFFF) / 1024.0 - 2.0;
    int4 chunk = world.origins[first.y >> 16];
    float blockSize = world.blockSize * float(1u << uint(chunk.w));
    float2 folded = float2(second.x & 0xFFF, (second.x >> 12) & 0xFFF) / 4095.0 * 2.0 - 1.0;
    float3 n = float3(folded.x, 1.0 - abs(folded.x) - abs(folded.y), folded.y);
    if (n.y < 0.0)
    {
        n.xz = (1.0 - abs(folded.yx)) * select(folded >= 0.0, float2(1.0), float2(-1.0));
    }
    BlockPoint b;
    b.world = world.corner + (float3(chunk.xyz * 32) + local) * blockSize;
    b.normal = normalize(n);
    b.surface = (second.y & 0xFFFF) | ((second.x >> 24) << 16);
    b.shade = 1.0;
    return b;
}

// Vertex `vertexId` of the world `push.level` names (its number and one): six to a quad, its quads one after
// another in the world's pool from `push.output` on; or, drawn smooth, a triangle's corner.
BlockPoint blockPoint(uint vertexId)
{
    BlockWorld world = buffers()->blockWorlds[push.level - 1];
    if ((world.flags & BLOCKS_SMOOTH) != 0)
    {
        return smoothPoint(world, vertexId);
    }
    uint2 quad = world.quads[push.output + vertexId / 6];
    uint first = quad.x;
    uint3 at = uint3(first & 31, (first >> 5) & 31, (first >> 10) & 31);
    float2 size = float2(((first >> 15) & 31) + 1, ((first >> 20) & 31) + 1);
    uint facing = (first >> 25) & 7;
    uint4 shades = (uint4(quad.y) >> uint4(8, 10, 12, 14)) & 3;
    bool flip = shades.x + shades.z < shades.y + shades.w;
    uint corner = flip ? BLOCK_FLIPPED[vertexId % 6] : BLOCK_CORNERS[vertexId % 6];
    uint3 axes = BLOCK_AXES[facing];
    bool out = (facing & 1) == 0;
    // Its face is on the block's far side along a facing it faces along, on its near side against one.
    float3 p = float3(at) + blockAxis(axes.x) * (out ? 1.0 : 0.0);
    p += blockAxis(axes.y) * (corner == 1 || corner == 2 ? size.x : 0.0);
    p += blockAxis(axes.z) * (corner >= 2 ? size.y : 0.0);
    // Its chunk's place, in its level's chunks; a coarser level's blocks as many times larger.
    int4 chunk = world.origins[quad.y >> 16];
    float blockSize = world.blockSize * float(1u << uint(chunk.w));
    BlockPoint b;
    b.world = world.corner + (float3(chunk.xyz * 32) + p) * blockSize;
    b.normal = blockAxis(axes.x) * (out ? 1.0 : -1.0);
    b.surface = quad.y & 0xFF;
    b.shade = float(shades[corner]) / 3.0;
    return b;
}

[shader("vertex")]
VertexOut blockVertex(uint vertexId : SV_VertexID)
{
    Frame* frame = push.frame;
    BlockPoint b = blockPoint(vertexId);
    BlockWorld world = buffers()->blockWorlds[push.level - 1];
    float4 at = float4(b.world, 1.0);
    VertexOut o;
    o.position = transform(frame->viewProj, at);
    o.worldPosition = b.world;
    o.normal = b.normal;
    o.tangent = float4(normalizeOr(float3(b.normal.y, -b.normal.x, 0.0), float3(1.0, 0.0, 0.0)), 1.0);
    // The corner's shading, for blockShading to darken by.
    o.uv = float2(b.shade, 0.0);
    o.material = world.material;
    o.lod = b.surface;
    o.instance = world.anchor;
    o.fade = 0;
    // Which world, for terrainShading (and a colour of its own in the meshlets view).
    o.cluster = push.level - 1;
    o.currentClip = transform(frame->unjitteredViewProj, at);
    o.previousClip = transform(frame->prevViewProj, at);
    o.moved = 0.0;
    o.hidden = 0.0;
    return o;
}

// Surface `index` of a world, its pictures from the side the ground faces.
TerrainLayer blockLayer(BlockWorld world, uint index, TerrainSpot spot, float3 normal, float4 tangent)
{
    TerrainSurface surface = world.surfaces[min(index, world.surfaceCount - 1)];
    surface.flags |= TERRAIN_SURFACE_TRIPLANAR;
    return terrainLayerAt(surface, spot, normal, tangent);
}

// A face's own surface in place of the world's material: its pictures from the side it faces, each block's own,
// darker where blocks close its corners in. Drawn smooth, its block's top where the ground faces up, its bottom where
// it faces down and its sides between, blended as the ground turns.
bool terrainShading(
    VertexOut input, inout float4 base, inout float metallic, inout float roughness, out float3 normal, inout float3 emission)
{
    normal = normalize(input.normal);
    BlockWorld world = buffers()->blockWorlds[input.cluster];
    float shade = lerp(0.35, 1.0, saturate(input.uv.x));
    if (world.surfaceCount == 0)
    {
        base.rgb *= shade;
        return false;
    }
    TerrainSpot spot;
    spot.place = input.worldPosition - world.corner;
    spot.dx = ddx(spot.place);
    spot.dy = ddy(spot.place);
    TerrainLayer layer;
    if ((world.flags & BLOCKS_SMOOTH) != 0)
    {
        uint3 surfaces = uint3(input.lod & 0xFF, (input.lod >> 8) & 0xFF, (input.lod >> 16) & 0xFF);
        // Wholly its top on slopes up to about 25 degrees, wholly its sides past about 50.
        float up = smoothstep(0.65, 0.9, normal.y);
        float down = smoothstep(0.65, 0.9, -normal.y);
        // The top or bottom where the ground faces that way, and the sides as it turns from it.
        uint toward = normal.y >= 0.0 ? surfaces.x : surfaces.z;
        float flat = max(up, down);
        layer = blockLayer(world, surfaces.y, spot, normal, input.tangent);
        if (flat > 0.0 && toward != surfaces.y)
        {
            TerrainLayer other = blockLayer(world, toward, spot, normal, input.tangent);
            layer.base = lerp(layer.base, other.base, flat);
            layer.metallic = lerp(layer.metallic, other.metallic, flat);
            layer.roughness = lerp(layer.roughness, other.roughness, flat);
            layer.normal = normalizeOr(lerp(layer.normal, other.normal, flat), normal);
        }
        else if (flat > 0.0)
        {
            layer = blockLayer(world, toward, spot, normal, input.tangent);
        }
    }
    else
    {
        layer = blockLayer(world, input.lod, spot, normal, input.tangent);
    }
    base = layer.base;
    base.rgb *= shade;
    metallic = layer.metallic;
    roughness = layer.roughness;
    normal = normalizeOr(layer.normal, normal);
    return true;
}

// A vertex in a sun cascade's view (push.list), placed as the camera's view places it.
[shader("vertex")]
float4 blockShadowVertex(uint vertexId : SV_VertexID) : SV_Position
{
    BlockPoint b = blockPoint(vertexId);
    return transform(shadowViewProj(push.frame, push.list, 0), float4(b.world, 1.0));
}
