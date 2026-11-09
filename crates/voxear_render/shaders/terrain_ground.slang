// A terrain's ground as shaders read it (src/terrain.rs): its heights,
// its holes, its surfaces' shares and pictures, and how thickly its grass
// grows, each sample in its tile's slot of the terrain's buffer. Included
// by terrain.slang, which draws the ground, grass.slang, which grows grass
// on it, and shading.slang, which blends models into it.

#pragma once

#include "common.slang"

// Which tile slot holds sample `sample` of the whole terrain, clamped to its edges, and where in it.
uint terrainSlot(Terrain terrain, int2 sample, out uint inside)
{
    int2 quads = int2(terrain.tiles * TERRAIN_TILE_QUADS);
    sample = clamp(sample, int2(0), quads);
    int2 tile = min(sample / int(TERRAIN_TILE_QUADS), int2(terrain.tiles) - 1);
    int2 at = sample - tile * int(TERRAIN_TILE_QUADS);
    inside = uint(at.y) * TERRAIN_TILE_SAMPLES + uint(at.x);
    return uint(tile.y) * terrain.tiles.x + uint(tile.x);
}

bool terrainHoleAt(Terrain terrain, int2 sample)
{
    uint inside;
    uint slot = terrainSlot(terrain, sample, inside);
    uint word = terrain.holes[slot * TERRAIN_HOLE_WORDS + inside / 32];
    return ((word >> (inside % 32)) & 1) != 0;
}

// Whether the ground at `local`, metres from the terrain's corner, is cut away: the triangle there has a corner on
// a hole's sample.
bool terrainCut(Terrain terrain, float2 local)
{
    if ((terrain.flags & TERRAIN_HOLES) == 0)
    {
        return false;
    }
    float2 quads = float2(terrain.tiles * TERRAIN_TILE_QUADS);
    float2 f = clamp(local / terrain.spacing, float2(0.0), quads);
    int2 i = min(int2(floor(f)), int2(quads) - 1);
    float2 t = f - float2(i);
    int2 third = t.y > t.x ? i + int2(0, 1) : i + int2(1, 0);
    return terrainHoleAt(terrain, i) || terrainHoleAt(terrain, i + int2(1, 1)) || terrainHoleAt(terrain, third);
}

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

// How thickly grass kind `kind` grows at sample `sample`, 0 to 1.
float terrainGrassAt(Terrain terrain, int2 sample, uint kind)
{
    uint inside;
    uint slot = terrainSlot(terrain, sample, inside);
    uint word = terrain.grass[(slot * TERRAIN_TILE_LEN + inside) * terrain.grassWords + kind / 4];
    return float((word >> (8 * (kind % 4))) & 0xFF) / 255.0;
}

// How thickly grass kind `kind` grows at `local`, metres from the terrain's corner, between the samples round it.
float terrainGrass(Terrain terrain, float2 local, uint kind)
{
    float2 quads = float2(terrain.tiles * TERRAIN_TILE_QUADS);
    float2 f = clamp(local / terrain.spacing, float2(0.0), quads);
    int2 i = min(int2(floor(f)), int2(quads) - 1);
    float2 t = f - float2(i);
    float top = lerp(terrainGrassAt(terrain, i, kind), terrainGrassAt(terrain, i + int2(1, 0), kind), t.x);
    float bottom = lerp(terrainGrassAt(terrain, i + int2(0, 1), kind), terrainGrassAt(terrain, i + int2(1, 1), kind), t.x);
    return lerp(top, bottom, t.y);
}

// The tracks at `world` (x and z in the world) from the map round the camera, blended between its four nearest
// texels: how deep the ground was pressed there, and when, the latest of the four. Nothing off the map.
float2 trackAt(Frame* frame, float2 world)
{
    Tracks tracks = frame->tracks;
    float2 at = world / tracks.texel - 0.5;
    int2 low = int2(floor(at));
    float2 within = at - float2(low);
    float2* texels = buffers()->tracks;
    int n = int(TRACK_TEXELS);
    float depth = 0.0;
    float pressed = -1e9;
    for (uint corner = 0; corner < 4; corner++)
    {
        int2 cell = low + int2(corner & 1, corner >> 1);
        int2 inMap = cell - tracks.origin;
        if (any(inMap < 0) || any(inMap >= n))
        {
            continue;
        }
        int2 slot = (cell % n + n) % n;
        float2 weights = lerp(1.0 - within, within, float2(corner & 1, corner >> 1));
        float2 texel = texels[slot.x + slot.y * TRACK_TEXELS];
        depth += texel.x * weights.x * weights.y;
        pressed = max(pressed, texel.y);
    }
    return float2(depth, pressed);
}

// Sample `sample`'s four surfaces' numbers, a byte each, and their shares out of 255.
uint2 terrainSharesAt(Terrain terrain, int2 sample)
{
    uint inside;
    uint slot = terrainSlot(terrain, sample, inside);
    return terrain.weights[slot * TERRAIN_TILE_LEN + inside];
}

// The surfaces at `local` and how much of each, the four largest, blended from the four samples round it.
void terrainShares(Terrain terrain, float2 local, out uint4 ids, out float4 shares)
{
    float2 quads = float2(terrain.tiles * TERRAIN_TILE_QUADS);
    float2 f = clamp(local / terrain.spacing, float2(0.0), quads);
    int2 i = min(int2(floor(f)), int2(quads) - 1);
    float2 t = f - float2(i);
    float4 corner = float4((1.0 - t.x) * (1.0 - t.y), t.x * (1.0 - t.y), (1.0 - t.x) * t.y, t.x * t.y);
    ids = uint4(0);
    shares = float4(0.0);
    for (uint c = 0; c < 4; c++)
    {
        uint2 packed = terrainSharesAt(terrain, i + int2(c & 1, c >> 1));
        for (uint k = 0; k < 4; k++)
        {
            float share = float((packed.y >> (8 * k)) & 0xFF) / 255.0 * corner[c];
            if (share <= 0.0)
            {
                continue;
            }
            uint id = (packed.x >> (8 * k)) & 0xFF;
            // Added to its own place, or put in the least's place if it is more.
            uint least = 0;
            bool placed = false;
            for (uint j = 0; j < 4; j++)
            {
                if (shares[j] > 0.0 && ids[j] == id)
                {
                    shares[j] += share;
                    placed = true;
                    break;
                }
                least = shares[j] < shares[least] ? j : least;
            }
            if (!placed && share > shares[least])
            {
                ids[least] = id;
                shares[least] = share;
            }
        }
    }
    float total = shares.x + shares.y + shares.z + shares.w;
    shares = total > 0.0 ? shares / total : float4(1.0, 0.0, 0.0, 0.0);
}

// A surface's pictures at a point of the ground.
struct TerrainLayer
{
    float4 base;
    float3 normal;
    float metallic;
    float roughness;
};

// Where a pixel is on the ground (x and z metres from the terrain's corner, y its height), and how that changes to
// the next pixel across and down. Its pictures are sampled with those changes, worked out once where every pixel
// of the quad is shading: in the branches over each surface the hardware's own would be undefined, and took the
// blurriest mip.
struct TerrainSpot
{
    float3 place;
    float3 dx;
    float3 dy;
};

// The plane a surface's pictures lie in: from above (0), along x (1) or along z (2).
float2 terrainSide(float3 v, uint side)
{
    return side == 0 ? float2(v.x, -v.z) : (side == 1 ? float2(v.z, -v.y) : float2(v.x, -v.y));
}

float4 terrainPicture(uint texture, TerrainSpot spot, uint side, float scale)
{
    float2 uv = terrainSide(spot.place, side) * scale;
    float2 dx = terrainSide(spot.dx, side) * scale;
    float2 dy = terrainSide(spot.dy, side) * scale;
    return textures[NonUniformResourceIndex(texture)].SampleGrad(samplers[SAMPLER_LINEAR_REPEAT], uv, dx, dy);
}

// How much a triplanar surface is seen from each side at `normal`: from above, along x, along z.
float3 terrainSides(float3 normal)
{
    float3 sides = pow(abs(normal.yxz), 4.0);
    return sides / max(sides.x + sides.y + sides.z, 1e-5);
}

// How high a surface's height picture stands at the spot, 0 to 1.
float terrainLayerHeight(TerrainSurface surface, TerrainSpot spot, float3 normal)
{
    Material material = buffers()->materials[surface.material];
    if ((surface.flags & TERRAIN_SURFACE_TRIPLANAR) == 0)
    {
        return terrainPicture(material.heightTexture, spot, 0, surface.uvScale).r;
    }
    float3 sides = terrainSides(normal);
    float height = 0.0;
    for (uint side = 0; side < 3; side++)
    {
        if (sides[side] > 0.01)
        {
            height += terrainPicture(material.heightTexture, spot, side, surface.uvScale).r * sides[side];
        }
    }
    return height;
}

// A normal map's bend at the spot in its own plane, x and y, as two-channel BC5 keeps them.
float2 terrainBend(Material material, TerrainSpot spot, uint side, float scale)
{
    if ((material.flags & MATERIAL_NORMAL_MAP) == 0)
    {
        return float2(0.0);
    }
    return (terrainPicture(material.normalTexture, spot, side, scale).xy * 2.0 - 1.0) * material.normalScale;
}

// One side of a surface's pictures, `weight` of it, added to `layer`.
void terrainAddSide(inout TerrainLayer layer, Material material, TerrainSpot spot, uint side, float scale, float weight)
{
    float4 metalRough = terrainPicture(material.metallicRoughnessTexture, spot, side, scale);
    layer.base += material.baseColor * terrainPicture(material.baseColorTexture, spot, side, scale) * weight;
    layer.metallic += material.metallic * metalRough.b * weight;
    layer.roughness += material.roughness * metalRough.g * weight;
}

// A surface's pictures at the spot, on ground facing `normal` with `tangent` along +x.
TerrainLayer terrainLayerAt(TerrainSurface surface, TerrainSpot spot, float3 normal, float4 tangent)
{
    Material material = buffers()->materials[surface.material];
    float scale = surface.uvScale;
    TerrainLayer layer;
    layer.base = float4(0.0);
    layer.metallic = 0.0;
    layer.roughness = 0.0;
    // Seen from above, bent along the ground's own tangent.
    float3 t = normalizeOr(tangent.xyz - normal * dot(normal, tangent.xyz), float3(1.0, 0.0, 0.0));
    float3 b = cross(normal, t) * tangent.w;
    if ((surface.flags & TERRAIN_SURFACE_TRIPLANAR) == 0)
    {
        terrainAddSide(layer, material, spot, 0, scale, 1.0);
        float2 bend = terrainBend(material, spot, 0, scale);
        layer.normal = normalizeOr(t * bend.x + b * bend.y + normal * sqrt(saturate(1.0 - dot(bend, bend))), normal);
        return layer;
    }
    // From each side as much as the ground faces it, each side's bend laid on the ground's normal there.
    float3 sides = terrainSides(normal);
    terrainAddSide(layer, material, spot, 0, scale, sides.x);
    float2 above = terrainBend(material, spot, 0, scale);
    float3 bent = (t * above.x + b * above.y + normal) * sides.x;
    if (sides.y > 0.01)
    {
        terrainAddSide(layer, material, spot, 1, scale, sides.y);
        float2 bend = terrainBend(material, spot, 1, scale);
        bent += float3(normal.x, bend.y + normal.y, bend.x + normal.z) * sides.y;
    }
    if (sides.z > 0.01)
    {
        terrainAddSide(layer, material, spot, 2, scale, sides.z);
        float2 bend = terrainBend(material, spot, 2, scale);
        bent += float3(bend.x + normal.x, bend.y + normal.y, normal.z) * sides.z;
    }
    layer.normal = normalizeOr(bent, normal);
    return layer;
}

// Whether `local`, metres from the terrain's corner, is in a dug column (src/dug.rs): where its heights are holes for
// its voxels to stand in, and grass still grows as the heights have it.
bool terrainDugColumn(Terrain terrain, float2 local)
{
    if (terrain.dugColumnsX == 0)
    {
        return false;
    }
    int2 column = int2(floor(local / (terrain.spacing * float(TERRAIN_DUG_COLUMN))));
    if (any(column < 0) || column.x >= int(terrain.dugColumnsX))
    {
        return false;
    }
    uint index = uint(column.y) * terrain.dugColumnsX + uint(column.x);
    return ((terrain.dugColumns[index / 32] >> (index % 32)) & 1) != 0;
}
