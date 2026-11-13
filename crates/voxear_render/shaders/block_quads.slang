// Where a world of blocks' faces lie (src/blocks.rs, voxear_terrain::blocks::mesh and ::smooth), as the shaders that
// draw them read the world's pool: blocks.slang its ground, water.slang its water. Each quad is two words: where it
// starts in its chunk, how many blocks it spans across and up its face, which way it faces, its surface, how dark each
// corner is, and its chunk's number in the world's pool, which finds where the chunk is. A quad is split along the
// diagonal between its lighter corners, so its shading runs smoothly across it.

#pragma once

#include "common.slang"

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
    // Whether the corner sways in the wind (voxear_terrain::blocks::mesh::swaying), and which corner of the world's
    // blocks it is, counted in blocks from the world's first: every face that meets there moves it alike.
    bool sways;
    int3 corner;
    // For a face that joins those of its block beside it (voxear_terrain::blocks::mesh::joins): which of the eight
    // round it, a bit each. For a face another block creeps over (mesh::mixing): that block's surface, then whether
    // it is a quad more over a face drawn already (the eighth bit), whether it creeps from corners rather than edges
    // (the ninth), and from which (four bits from the tenth). Over either, from the fourteenth bit, the level of
    // its chunk: how many times its blocks were doubled.
    uint joins;
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
    b.sways = false;
    b.corner = 0;
    b.joins = 0;
    return b;
}

// The surfaces of corner `corner` of a world drawn smooth, counted from `push.output` on: its ground's top, sides and
// bottom, a byte each.
uint smoothSurfaces(BlockWorld world, uint corner)
{
    uint2 second = world.quads[push.output + corner * 2 + 1];
    return (second.y & 0xFFFF) | ((second.x >> 24) << 16);
}

// Vertex `vertexId` of a quad of `world`: six to a quad, its quads one after another in the world's pool from
// `push.output` on.
BlockPoint cubePoint(BlockWorld world, uint vertexId)
{
    uint2 quad = world.quads[push.output + vertexId / 6];
    uint first = quad.x;
    uint3 at = uint3(first & 31, (first >> 5) & 31, (first >> 10) & 31);
    float2 size = float2(((first >> 15) & 31) + 1, ((first >> 20) & 31) + 1);
    // A face drawn with a material whose pictures are a joining face's five tiles is one block, and keeps the faces
    // round it that it joins where its size would be.
    // And so is a face drawn with a look's twin, which keeps what creeps over it there and where its swaying corners
    // would be: it sways at none.
    uint joins = 0;
    uint swaying = first >> 28;
    if (world.surfaceCount > 0)
    {
        TerrainSurface drawn = world.surfaces[min(quad.y & 0xFF, world.surfaceCount - 1)];
        if ((drawn.flags & TERRAIN_SURFACE_MIXED) != 0)
        {
            joins = ((first >> 15) & 0x3FF) | (swaying << 10);
            size = float2(1.0, 1.0);
            swaying = 0;
        }
        else if ((buffers()->materials[drawn.material].flags & MATERIAL_CONNECTED) != 0)
        {
            joins = (first >> 15) & 0xFF;
            size = float2(1.0, 1.0);
        }
    }
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
    // Only the world's own blocks sway: a far ring's are too large to.
    b.sways = chunk.w == 0 && ((swaying >> corner) & 1) != 0;
    b.corner = chunk.xyz * 32 + int3(p);
    b.joins = joins | (uint(chunk.w) << 14);
    return b;
}

// Vertex `vertexId` of the world `push.level` names (its number and one): a quad's corner, or, drawn smooth, a
// triangle's.
BlockPoint blockPoint(uint vertexId)
{
    BlockWorld world = buffers()->blockWorlds[push.level - 1];
    if ((world.flags & BLOCKS_SMOOTH) != 0)
    {
        return smoothPoint(world, vertexId);
    }
    return cubePoint(world, vertexId);
}
