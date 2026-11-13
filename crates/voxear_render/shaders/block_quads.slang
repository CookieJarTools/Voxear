// Where a world of blocks' faces lie (src/blocks.rs, voxear_terrain::blocks::mesh and ::smooth), as the shaders that
// draw them read the world's pool: blocks.slang its ground, water.slang its water. Each quad is two words: where it
// starts in its chunk, how many blocks it spans across and up its face, which way it faces, its surface, how dark each
// corner is, and its chunk's number in the world's pool, which finds where the chunk is. A quad is split along the
// diagonal between its lighter corners, so its shading runs smoothly across it. A loose quad is no face of a cube:
// a sheet of a bushy block's leaves, or of a vine (loosePoint).

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
    // For a loose quad, a sheet of bushy leaves or of a vine: which it is (BLOCK_TUFT or BLOCK_VINE, 0 for a face),
    // which of its kind's sheets with the vine's foot flag, how open its block is from 0 to 3, a number of its own
    // for its pattern, where the corner is across and up its sheet from 0 to 1, and how much further than a
    // block's corner it sways.
    uint loose;
    uint sheet;
    uint open;
    uint seed;
    float2 across;
    float reach;
};

// What a loose quad is, where a face says which way it faces (voxear_terrain::blocks::mesh::TUFT and VINE).
static const uint BLOCK_TUFT = 6;
static const uint BLOCK_VINE = 7;
// A vine's sheet at the foot of its vine (mesh::VINE_TIP).
static const uint BLOCK_VINE_TIP = 8;

// How far from the camera loose quads are drawn, in blocks (src/blocks.rs, LOOSE_REACH): they shrink away over the
// last quarter of it.
static const float BLOCK_LOOSE_REACH = 160.0;

// How far a sheet of leaves reaches from its block's middle along each axis, in blocks: nearly twice as far as the
// block's own faces, thinning out over its outer half (blockSheetLook).
static const float BLOCK_TUFT_REACH = 0.95;

// How far a vine's sheet stands off the block it clings to, in blocks.
static const float BLOCK_VINE_OFF = 0.06;

// The sheets of leaves of a bushy block, each by the way it runs across and the way it runs up from the block's
// middle (mesh::TUFT_SHEETS): two upright from corner to corner, four that lean.
static const float3 BLOCK_TUFT_ACROSS[6] = {
    float3(1, 0, 1), float3(1, 0, -1), float3(1, 0, 0), float3(1, 0, 0), float3(0, 0, 1), float3(0, 0, 1)
};
static const float3 BLOCK_TUFT_UP[6] = {
    float3(0, 1, 0), float3(0, 1, 0), float3(0, 1, 1), float3(0, 1, -1), float3(1, 1, 0), float3(-1, 1, 0)
};

// A number for a block of the world and `salt`, from whole numbers alone.
uint blockNumber(int3 block, uint salt)
{
    uint h = uint(block.x) * 0x9E3779B1u ^ uint(block.y) * 0x85EBCA77u ^ uint(block.z) * 0xC2B2AE3Du ^ salt * 0x27D4EB2Fu;
    h ^= h >> 15;
    h *= 0x2C1B3C6Du;
    h ^= h >> 12;
    h *= 0x297A2D39u;
    h ^= h >> 15;
    return h;
}

// How many blocks a loose quad's sheet measures across and up: a sheet of leaves half as large again as its
// block, longer along a diagonal; a vine's as its block, or its diagonal where it hangs free.
float2 blockSheetSize(uint loose, uint sheet)
{
    if (loose == BLOCK_TUFT)
    {
        float reach = BLOCK_TUFT_REACH * 2.0;
        return sheet < 2 ? float2(reach * 1.41421356, reach) : float2(reach, reach * 1.41421356);
    }
    return float2((sheet & 7) < 4 ? 1.0 : 1.41421356, 1.0);
}

// Corner `corner` of a loose quad (voxear_terrain::blocks::mesh::ChunkMesh::loose) of the block at `at` in its
// chunk: a sheet of a bushy block's leaves, crossed through the block's middle and reaching out past it, or a
// sheet of a vine. Seen from both sides: its corners are taken the other way round where `eye` is behind it, so
// it is never turned away. It shrinks to nothing over the last quarter of the way to where loose quads end.
BlockPoint loosePoint(BlockWorld world, uint2 quad, uint vertex, float3 eye)
{
    uint first = quad.x;
    uint3 at = uint3(first & 31, (first >> 5) & 31, (first >> 10) & 31);
    uint kind = (first >> 25) & 7;
    uint sheet = (first >> 15) & 1023;
    int4 chunk = world.origins[quad.y >> 16];
    int3 block = chunk.xyz * 32 + int3(at);
    float3 middle = float3(at) + 0.5;
    float3 across;
    float3 up;
    float3 from;
    float reach = 1.0;
    if (kind == BLOCK_TUFT)
    {
        // Each block's sheets a little off its middle and a size of their own, so a crown is no lattice.
        uint number = blockNumber(block, 11);
        float3 off = float3(number & 255, (number >> 8) & 255, (number >> 16) & 255) / 255.0 - 0.5;
        float size = BLOCK_TUFT_REACH * (0.88 + 0.24 * float(number >> 24) / 255.0);
        from = middle + off * 0.14;
        across = BLOCK_TUFT_ACROSS[min(sheet, 5u)] * size;
        up = BLOCK_TUFT_UP[min(sheet, 5u)] * size;
        reach = 1.6;
    }
    else
    {
        uint side = sheet & 7;
        up = float3(0.0, 0.5, 0.0);
        from = middle;
        if (side < 4)
        {
            // Against the block beside it along +x, -x, +z or -z.
            float3 toward = side < 2 ? float3(side == 0 ? 1.0 : -1.0, 0.0, 0.0) : float3(0.0, 0.0, side == 2 ? 1.0 : -1.0);
            from += toward * (0.5 - BLOCK_VINE_OFF);
            across = float3(-toward.z, 0.0, toward.x) * 0.5;
        }
        else
        {
            across = float3(0.5, 0.0, side == 4 ? 0.5 : -0.5);
        }
    }
    float3 facing = normalize(cross(across, up));
    float blockSize = world.blockSize;
    float3 centre = world.corner + (float3(chunk.xyz * 32) + from) * blockSize;
    // Shrunk away before the distance loose quads stop at.
    float away = length(centre - eye) / (BLOCK_LOOSE_REACH * blockSize);
    float shown = kind == BLOCK_TUFT ? 1.0 - smoothstep(0.75, 1.0, away) : 1.0;
    // Its corners in order, or the other way round seen from behind.
    static const uint ORDER[6] = { 0, 1, 2, 0, 2, 3 };
    static const uint TURNED[6] = { 0, 2, 1, 0, 3, 2 };
    bool behind = dot(eye - centre, facing) < 0.0;
    uint corner = behind ? TURNED[vertex] : ORDER[vertex];
    float2 sign = float2(corner == 1 || corner == 2 ? 1.0 : -1.0, corner >= 2 ? 1.0 : -1.0);
    float3 offset = (across * sign.x + up * sign.y) * shown;
    BlockPoint b;
    b.world = centre + offset * blockSize;
    // Lit as a soft mass: leaves from their block's middle outwards and up, a vine as it faces, and up.
    float3 toEye = behind ? -facing : facing;
    b.normal = kind == BLOCK_TUFT ? normalize(normalize(offset + float3(0.0, 1e-3, 0.0)) * 0.35 + float3(0.0, 1.0, 0.0))
                                  : normalize(toEye * 0.7 + float3(0.0, 0.6, 0.0));
    b.surface = quad.y & 0xFF;
    b.shade = 1.0;
    // The corner of its block's cube nearest it moves it, as that corner moves the block's own faces.
    int3 nearest = int3(offset.x > 0.0 ? 1 : 0, offset.y > 0.0 ? 1 : 0, offset.z > 0.0 ? 1 : 0);
    b.sways = chunk.w == 0 && ((first >> (28 + corner)) & 1) != 0;
    b.corner = block + nearest;
    b.loose = kind;
    b.sheet = sheet & 15;
    b.open = (quad.y >> 8) & 3;
    // A vine's pattern runs down its whole length: its number is its column's alone.
    b.seed = blockNumber(kind == BLOCK_TUFT ? block : int3(block.x, 0, block.z), 3 + (sheet & 7)) & 0xFFFF;
    b.across = sign * 0.5 + 0.5;
    b.reach = reach;
    return b;
}

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
    b.loose = 0;
    b.sheet = 0;
    b.open = 3;
    b.seed = 0;
    b.across = 0.0;
    b.reach = 1.0;
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
BlockPoint cubePoint(BlockWorld world, uint vertexId, bool loose = false)
{
    uint2 quad = world.quads[push.output + vertexId / 6];
    uint first = quad.x;
    uint3 at = uint3(first & 31, (first >> 5) & 31, (first >> 10) & 31);
    float2 size = float2(((first >> 15) & 31) + 1, ((first >> 20) & 31) + 1);
    uint facing = (first >> 25) & 7;
    if (facing >= BLOCK_TUFT && loose)
    {
        return loosePoint(world, quad, vertexId % 6, push.frame->cameraPosition);
    }
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
    b.sways = chunk.w == 0 && ((first >> (28 + corner)) & 1) != 0;
    b.corner = chunk.xyz * 32 + int3(p);
    b.loose = 0;
    b.sheet = 0;
    b.open = 3;
    b.seed = 0;
    b.across = 0.0;
    b.reach = 1.0;
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
    return cubePoint(world, vertexId, true);
}
