// Worlds of blocks (src/blocks.rs): each quad of a chunk's faces two words (voxear_terrain::blocks::mesh), six
// vertices made of it here from their number, as no vertex buffer holds them: where it starts in its chunk, how many
// blocks it spans across and up its face, which way it faces, its surface, how dark each corner is, and its chunk's
// number in the world's pool, which finds where the chunk is. A quad is split along the diagonal between its lighter
// corners, so its shading runs smoothly across it.
//
// Its faces are shaded by shading.slang's fragment shaders, as the terrain's are: each face with its surface's
// material, its pictures repeated each block, darker at the corners blocks close in.

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

// Vertex `vertexId` of the world `push.level` names (its number and one): six to a quad, its quads one after
// another in the world's pool from `push.output` on.
BlockPoint blockPoint(uint vertexId)
{
    BlockWorld world = buffers()->blockWorlds[push.level - 1];
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
    int3 chunk = world.origins[quad.y >> 16].xyz;
    BlockPoint b;
    b.world = world.corner + (float3(chunk * 32) + p) * world.blockSize;
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

// A face's own surface in place of the world's material: its pictures from the side it faces, each block's own,
// darker where blocks close its corners in.
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
    TerrainSurface surface = world.surfaces[min(input.lod, world.surfaceCount - 1)];
    surface.flags |= TERRAIN_SURFACE_TRIPLANAR;
    TerrainSpot spot;
    spot.place = input.worldPosition - world.corner;
    spot.dx = ddx(spot.place);
    spot.dy = ddy(spot.place);
    TerrainLayer layer = terrainLayerAt(surface, spot, normal, input.tangent);
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
