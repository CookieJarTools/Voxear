// A terrain's ground as shaders read it (src/terrain.rs): its heights,
// its holes and how thickly its grass grows, each sample in its tile's slot
// of the terrain's buffer. Included by terrain.slang, which draws the
// ground, and grass.slang, which grows grass on it.

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
