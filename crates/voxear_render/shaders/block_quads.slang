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

// Vertex `vertexId` of a quad of `world`: six to a quad, its quads one after another in the world's pool from
// `push.output` on.
BlockPoint cubePoint(BlockWorld world, uint vertexId)
{
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
