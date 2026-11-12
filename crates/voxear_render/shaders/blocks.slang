// Worlds of blocks (src/blocks.rs): each quad of a chunk's faces two words (voxear_terrain::blocks::mesh), six
// vertices made of it here from their number, as no vertex buffer holds them: where it starts in its chunk, how many
// blocks it spans across and up its face, which way it faces, its surface, how dark each corner is, and its chunk's
// number in the world's pool, which finds where the chunk is. A quad is split along the diagonal between its lighter
// corners, so its shading runs smoothly across it.
//
// Its faces are shaded by shading.slang's fragment shaders, as the terrain's are: each face with its surface's
// material, its pictures repeated each block, darker at the corners blocks close in. A face whose material is cut
// out is drawn only where its picture is solid, in the depth pass as in the shaded one.
//
// A world drawn smooth (voxear_terrain::blocks::smooth) holds triangles' corners in place of quads, two pairs of words
// each, a vertex each: where it is in its chunk and the chunk's number, then the way the ground faces there and the
// surfaces of the ground there for its top, sides and bottom, which its fragments blend by the way they face. A
// triangle's corners may be of three kinds of ground: its fragments blend them by how near each corner they are.

#define TERRAIN_SHADING
#include "shading.slang"
#include "block_quads.slang"

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
    if ((world.flags & BLOCKS_SMOOTH) != 0)
    {
        // Every vertex of a triangle carries all three corners' surfaces: the first's top, sides and bottom and the
        // second's top beside them, the second's sides and the third's top and sides over the world's number; and,
        // in place of a corner's shading, how much of the first and of the second corner it is.
        uint first = vertexId - vertexId % 3;
        uint a = smoothSurfaces(world, first);
        uint second = smoothSurfaces(world, first + 1);
        uint third = smoothSurfaces(world, first + 2);
        o.lod = (a & 0xFFFFFF) | ((second & 0xFF) << 24);
        o.cluster |= (((second >> 8) & 0xFF) << 8) | ((third & 0xFFFF) << 16);
        o.uv = float2(vertexId % 3 == 0 ? 1.0 : 0.0, vertexId % 3 == 1 ? 1.0 : 0.0);
    }
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

// One layer blended into another, `share` of it.
void blendLayer(inout TerrainLayer layer, TerrainLayer other, float share)
{
    layer.base = lerp(layer.base, other.base, share);
    layer.metallic = lerp(layer.metallic, other.metallic, share);
    layer.roughness = lerp(layer.roughness, other.roughness, share);
    layer.normal = lerp(layer.normal, other.normal, share);
}

// Ground of a kind drawn smooth, by its surfaces (top, sides, bottom): its top where the ground faces up, its bottom
// where it faces down and its sides between, blended as the ground turns.
TerrainLayer smoothLayer(BlockWorld world, uint3 surfaces, TerrainSpot spot, float3 normal, float4 tangent)
{
    // Wholly its top on slopes up to about 25 degrees, wholly its sides past about 50.
    float up = smoothstep(0.65, 0.9, normal.y);
    float down = smoothstep(0.65, 0.9, -normal.y);
    // The top or bottom where the ground faces that way, and the sides as it turns from it.
    uint toward = normal.y >= 0.0 ? surfaces.x : surfaces.z;
    float flat = max(up, down);
    if (flat >= 1.0 || toward == surfaces.y)
    {
        return blockLayer(world, toward, spot, normal, tangent);
    }
    TerrainLayer layer = blockLayer(world, surfaces.y, spot, normal, tangent);
    if (flat > 0.0)
    {
        blendLayer(layer, blockLayer(world, toward, spot, normal, tangent), flat);
    }
    return layer;
}

// A face's own surface in place of the world's material: its pictures from the side it faces, each block's own,
// darker where blocks close its corners in. Drawn smooth, each of its triangle's corners' kinds of ground as much as
// the fragment is that corner's, a little sharpened, so sand meets grass over a soft edge rather than along the
// triangles' own.
bool terrainShading(
    VertexOut input, inout float4 base, inout float metallic, inout float roughness, out float3 normal, inout float3 emission)
{
    normal = normalize(input.normal);
    BlockWorld world = buffers()->blockWorlds[input.cluster & 0xFF];
    bool smooth = (world.flags & BLOCKS_SMOOTH) != 0;
    float shade = smooth ? 1.0 : lerp(0.35, 1.0, saturate(input.uv.x));
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
    if (smooth)
    {
        // Each corner's top and sides (blockVertex); the first's bottom stands for all three.
        uint3 tops = uint3(input.lod & 0xFF, input.lod >> 24, (input.cluster >> 16) & 0xFF);
        uint3 sides = uint3((input.lod >> 8) & 0xFF, (input.cluster >> 8) & 0xFF, input.cluster >> 24);
        uint bottom = (input.lod >> 16) & 0xFF;
        layer = smoothLayer(world, uint3(tops.x, sides.x, bottom), spot, normal, input.tangent);
        bool second = tops.y != tops.x || sides.y != sides.x;
        bool third = tops.z != tops.x || sides.z != sides.x;
        if (second || third)
        {
            float3 share = saturate(float3(input.uv.x, input.uv.y, 1.0 - input.uv.x - input.uv.y));
            share *= share;
            share /= max(share.x + share.y + share.z, 1e-5);
            TerrainLayer first = layer;
            TerrainLayer other = first;
            if (second)
            {
                other = smoothLayer(world, uint3(tops.y, sides.y, bottom), spot, normal, input.tangent);
            }
            TerrainLayer last = first;
            if (third && second && tops.z == tops.y && sides.z == sides.y)
            {
                last = other;
            }
            else if (third)
            {
                last = smoothLayer(world, uint3(tops.z, sides.z, bottom), spot, normal, input.tangent);
            }
            layer.base = first.base * share.x + other.base * share.y + last.base * share.z;
            layer.metallic = first.metallic * share.x + other.metallic * share.y + last.metallic * share.z;
            layer.roughness = first.roughness * share.x + other.roughness * share.y + last.roughness * share.z;
            layer.normal = first.normal * share.x + other.normal * share.y + last.normal * share.z;
        }
    }
    else
    {
        layer = blockLayer(world, input.lod, spot, normal, input.tangent);
        // A face whose material is cut out, as leaves are, is not there where its picture is clear.
        float cutoff = buffers()->materials[world.surfaces[min(input.lod, world.surfaceCount - 1)].material].alphaCutoff;
        if (cutoff > 0.0 && layer.base.a < cutoff)
        {
            discard;
        }
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
