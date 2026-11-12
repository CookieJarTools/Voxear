// Worlds of blocks (src/blocks.rs): each quad of a chunk's faces two words (voxear_terrain::blocks::mesh), six
// vertices made of it here from their number, as no vertex buffer holds them: where it starts in its chunk, how many
// blocks it spans across and up its face, which way it faces, its surface, how dark each corner is, and its chunk's
// number in the world's pool, which finds where the chunk is. A quad is split along the diagonal between its lighter
// corners, so its shading runs smoothly across it.
//
// Its faces are shaded by shading.slang's fragment shaders, as the terrain's are: each face with its surface's
// material, its pictures repeated each block, darker at the corners blocks close in. A face whose material is cut
// out is drawn only where its picture is solid, in the depth pass as in the shaded one, and casts a shadow only there.
//
// A world drawn smooth (voxear_terrain::blocks::smooth) holds triangles' corners in place of quads, two pairs of words
// each, a vertex each: where it is in its chunk and the chunk's number, then the way the ground faces there and the
// surfaces of the ground there for its top, sides and bottom, which its fragments blend by the way they face. A
// triangle's corners may be of three kinds of ground: its fragments blend them by how near each corner they are.

#define TERRAIN_SHADING
#include "shading.slang"
#include "block_quads.slang"

// How far a swaying corner moves at wind strength 1, as a share of a block, and the furthest it ever does.
static const float BLOCK_SWAY = 0.07;
static const float BLOCK_SWAY_MOST = 0.22;

// A number from 0 to 1 for a corner of the world's blocks, from whole numbers alone.
float blockCornerPhase(int3 corner)
{
    uint h = uint(corner.x) * 0x9E3779B1u ^ uint(corner.y) * 0x85EBCA77u ^ uint(corner.z) * 0xC2B2AE3Du;
    h ^= h >> 15;
    h *= 0x2C1B3C6Du;
    h ^= h >> 12;
    h *= 0x297A2D39u;
    h ^= h >> 15;
    return float(h >> 8) / 16777216.0;
}

// How far the wind moves a corner of a block that sways, as leaves do, at `time`: downwind as hard as the wind and
// the gust over it blow, to and fro, across and up and down at a pace of the corner's own, so a crown of leaves
// stirs all over while a gust still crosses it whole. Every face that meets at the corner moves it alike, and a
// corner a still block touches does not sway (voxear_terrain::blocks::mesh), so nothing parts.
float3 blockSway(Frame* frame, BlockWorld world, BlockPoint b, float time)
{
    Wind wind = frame->wind;
    if (!b.sways || wind.strength <= 0.0)
    {
        return 0.0;
    }
    float3 along = float3(wind.direction.x, 0.0, wind.direction.y);
    float3 across = float3(-wind.direction.y, 0.0, wind.direction.x);
    float push = max(windPush(wind, levelPosition(frame, b.world, 4096.0).xz, time), 0.0);
    float phase = blockCornerPhase(b.corner) * 6.2831853;
    float stir = 0.4 + 0.6 * saturate(wind.turbulence);
    float to = sin(time * 2.1 + phase) * 0.6 + sin(time * 3.7 + phase * 1.7) * 0.4;
    float side = sin(time * 2.9 + phase * 2.3);
    float bob = sin(time * 3.3 + phase * 1.3);
    float reach = min(BLOCK_SWAY * push, BLOCK_SWAY_MOST) * world.blockSize;
    float3 swayed = along * (0.5 + 0.5 * to * stir) + across * (side * 0.45 * stir);
    swayed.y += bob * 0.3 * stir;
    return swayed * reach;
}

[shader("vertex")]
VertexOut blockVertex(uint vertexId : SV_VertexID)
{
    Frame* frame = push.frame;
    BlockPoint b = blockPoint(vertexId);
    BlockWorld world = buffers()->blockWorlds[push.level - 1];
    // Leaves sway, now and as they were a frame ago, for their motion.
    float3 swayed = blockSway(frame, world, b, frame->time);
    float3 before = b.world + blockSway(frame, world, b, frame->time - frame->deltaTime);
    b.world += swayed;
    float4 at = float4(b.world, 1.0);
    VertexOut o;
    o.position = transform(frame->viewProj, at);
    o.worldPosition = b.world;
    o.normal = b.normal;
    o.tangent = float4(normalizeOr(float3(b.normal.y, -b.normal.x, 0.0), float3(1.0, 0.0, 0.0)), 1.0);
    // The corner's shading, for blockShading to darken by, and the faces round it a face that joins is joined to.
    o.uv = float2(b.shade, float(b.joins));
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
    o.previousClip = transform(frame->prevViewProj, float4(before, 1.0));
    // Bounced light is read where the block stands.
    o.moved = swayed;
    o.hidden = 0.0;
    return o;
}

// For a face that joins those of its block beside it, where on its surface's pictures the spot is read. The pictures
// are five tiles side by side: the face alone, its border all round; filled, with none; joined left and right;
// joined up and down; and joined all round with its inner corners showing. Each quarter of the face is that quarter
// of the tile its own three neighbours make it: the one beside it, the one above or below it, and the one between
// them. `joins` says which of the eight faces round it the face is joined to (voxear_terrain::blocks::mesh::JOINS),
// along its picture as terrainSide lays it. Any other face's spot comes back as it is.
TerrainSpot blockJoined(BlockWorld world, uint index, TerrainSpot spot, float3 normal, uint joins)
{
    TerrainSurface surface = world.surfaces[min(index, world.surfaceCount - 1)];
    Material material = buffers()->materials[surface.material];
    if ((material.flags & MATERIAL_CONNECTED) == 0)
    {
        return spot;
    }
    uint side = abs(normal.y) > 0.5 ? 0 : (abs(normal.x) > 0.5 ? 1 : 2);
    float2 within = frac(terrainSide(spot.place, side) * surface.uvScale);
    bool right = within.x >= 0.5;
    bool lower = within.y >= 0.5;
    bool beside = ((joins >> (right ? 1 : 0)) & 1) != 0;
    bool above = ((joins >> (lower ? 3 : 2)) & 1) != 0;
    bool between = ((joins >> (4 + (right ? 1 : 0) + (lower ? 2 : 0))) & 1) != 0;
    float tile = beside ? (above ? (between ? 1.0 : 4.0) : 2.0) : (above ? 3.0 : 0.0);
    // Half a pixel in from the tile's sides, so a smoothed picture takes nothing from the tile beside it.
    uint width, height;
    textures[NonUniformResourceIndex(material.baseColorTexture)].GetDimensions(width, height);
    float edge = 2.5 / float(max(width, 5));
    float2 read = float2((tile + clamp(within.x, edge, 1.0 - edge)) / 5.0, within.y) / surface.uvScale;
    // Back to a place terrainSide reads there, and a fifth as fast along the tiles.
    TerrainSpot joined = spot;
    if (side == 0)
    {
        joined.place.x = read.x;
        joined.place.z = -read.y;
        joined.dx.x *= 0.2;
        joined.dy.x *= 0.2;
    }
    else if (side == 1)
    {
        joined.place.z = read.x;
        joined.place.y = -read.y;
        joined.dx.z *= 0.2;
        joined.dy.z *= 0.2;
    }
    else
    {
        joined.place.x = read.x;
        joined.place.y = -read.y;
        joined.dx.x *= 0.2;
        joined.dy.x *= 0.2;
    }
    return joined;
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

// A number from 0 to 1 for a small cube, from whole numbers alone.
float blockGrain(int3 cube)
{
    uint h = uint(cube.x) * 0x8DA6B343u ^ uint(cube.y) * 0xD8163841u ^ uint(cube.z) * 0xCB1AB31Fu;
    h ^= h >> 16;
    h *= 0x7FEB352Du;
    h ^= h >> 15;
    h *= 0x846CA68Bu;
    h ^= h >> 16;
    return float(h >> 8) / 16777216.0;
}

// The most small cubes deep a face is cut, and the most of them a ray is walked across.
static const int BLOCK_RELIEF_DEEPEST = 4;
static const int BLOCK_CUBE_STEPS = 12;

// A cube's face drawn as made of small cubes (src/blocks.rs, BlockVoxels): `world.voxels` of them along a block's
// side, in a grid over the whole world, so faces side by side and round a corner agree. Each small cube is one
// colour, the face's picture's there, and stands a whole number of small cubes below the face: deeper the darker
// its part of the picture is than the picture is all over, or for a face with no picture, here and there. The ray
// from the camera is walked across the grid from where it meets the face until it meets a small cube's top or the
// side of the next, which gives the face's look its place to read the pictures at (`spot`, with a small cube's
// width for the pixel's, so the picture is read one colour a small cube), the way the small cube's own face looks
// (`facing`), how many small cubes down it is (`sunk`) and a number of its own (`grain`). Nothing is cut out and no
// depth is written: the small cubes are in the shading alone. `shown` is how much of this the pixel takes, less
// as the small cubes get too small on screen to tell apart, where the plain face takes over.
bool blockCubes(
    BlockWorld world,
    uint index,
    float3 at,
    float3 normal,
    inout TerrainSpot spot,
    out float3 facing,
    out float sunk,
    out float grain,
    out float shown)
{
    facing = normal;
    sunk = 0.0;
    grain = 0.5;
    shown = 0.0;
    if (world.voxels < 1.0)
    {
        return false;
    }
    // A face that joins those beside it is drawn plain: its pictures are tiles, not one face's.
    if ((buffers()->materials[world.surfaces[min(index, world.surfaceCount - 1)].material].flags & MATERIAL_CONNECTED) != 0)
    {
        return false;
    }
    float cube = world.blockSize / world.voxels;
    shown = 1.0 - smoothstep(0.4, 0.8, max(length(spot.dx), length(spot.dy)) / cube);
    if (shown <= 0.0)
    {
        return false;
    }
    // The face's own axis, and the two it lies along.
    uint axis = abs(normal.x) > 0.5 ? 0 : (abs(normal.y) > 0.5 ? 1 : 2);
    uint u = (axis + 1) % 3;
    uint v = (axis + 2) % 3;
    uint side = axis == 1 ? 0 : (axis == 0 ? 1 : 2);
    float3 alongU = float3(0.0);
    float3 alongV = float3(0.0);
    alongU[u] = 1.0;
    alongV[v] = 1.0;
    float3 grid = spot.place / cube;
    // The layer of small cubes just under the face.
    int layer = int(round(grid[axis])) - (normal[axis] > 0.0 ? 1 : 0);
    float3 ray = normalize(at - push.frame->cameraPosition);
    float2 across = float2(ray[u], ray[v]);
    // How fast the ray goes down into the face: never so slowly that it runs along it for ever.
    float down = max(-dot(ray, normal), 0.05);

    TerrainSurface surface = world.surfaces[min(index, world.surfaceCount - 1)];
    Material material = buffers()->materials[surface.material];
    uint width, height;
    textures[NonUniformResourceIndex(material.baseColorTexture)].GetDimensions(width, height);
    bool pictured = width > 2;
    int deepest = clamp(int(round(world.relief)), 0, BLOCK_RELIEF_DEEPEST);
    // The picture's brightness over a whole block, which each small cube's is set against.
    TerrainSpot whole = spot;
    whole.dx = alongU * world.blockSize;
    whole.dy = alongV * world.blockSize;
    float3 weights = float3(0.2126, 0.7152, 0.0722);
    float3 all = pictured ? terrainPicture(material.baseColorTexture, whole, side, surface.uvScale).rgb : float3(0.0);
    float usual = dot(all, weights);

    TerrainSpot small;
    small.place = spot.place;
    small.dx = alongU * cube;
    small.dy = alongV * cube;
    float2 from = float2(grid[u], grid[v]);
    int2 here = int2(floor(from));
    int2 toward = int2(across.x >= 0.0 ? 1 : -1, across.y >= 0.0 ? 1 : -1);
    float2 span = 1.0 / max(abs(across), 1e-6);
    float2 next = (float2(here + max(toward, int2(0, 0))) - from) * float2(toward) * span;
    float travelled = 0.0;
    for (int i = 0; i < BLOCK_CUBE_STEPS; i++)
    {
        // How far down this small cube's top is.
        int3 key = int3(0, 0, 0);
        key[u] = here.x;
        key[v] = here.y;
        key[axis] = layer;
        float own = blockGrain(key + int3(int(index) * 7919, 0, 0));
        small.place[u] = (float(here.x) + 0.5) * cube;
        small.place[v] = (float(here.y) + 0.5) * cube;
        float low;
        if (pictured)
        {
            float bright = dot(terrainPicture(material.baseColorTexture, small, side, surface.uvScale).rgb, weights);
            low = saturate((usual - bright) / max(usual, 0.04) * 2.5);
        }
        else
        {
            // Without a picture, about a fifth of the small cubes are sunk, some deeper than others, more of them
            // together in some places than others: each small cube's own number and that of the three by three
            // it is one of.
            int3 patch = int3(floor(float3(key) / 3.0));
            float rough = own * 0.55 + blockGrain(patch + int3(int(index) * 7919, 31, 0)) * 0.45;
            low = saturate((rough - 0.62) * 4.0);
        }
        float top = floor(low * float(deepest) + 0.5);
        if (i > 0 && travelled * down > top)
        {
            // The ray came in below this small cube's top: it meets its side. `facing` was set as it stepped.
            sunk = travelled * down;
            grain = own;
            break;
        }
        float leaves = min(next.x, next.y);
        if (top <= leaves * down || i == BLOCK_CUBE_STEPS - 1)
        {
            // It reaches this small cube's top before it leaves it.
            facing = normal;
            sunk = top;
            grain = own;
            break;
        }
        travelled = leaves;
        if (next.x < next.y)
        {
            here.x += toward.x;
            next.x += span.x;
            facing = -alongU * float(toward.x);
        }
        else
        {
            here.y += toward.y;
            next.y += span.y;
            facing = -alongV * float(toward.y);
        }
    }
    // The pictures are read at the small cube's middle, a small cube wide; the plain face's place and width as
    // the small cubes give way to it.
    spot.place = lerp(spot.place, small.place, shown);
    spot.dx = lerp(spot.dx, small.dx, shown);
    spot.dy = lerp(spot.dy, small.dy, shown);
    return true;
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
    // Where the face stands, not where the wind has swayed it to: its picture moves with it.
    float3 stands = input.worldPosition - input.moved;
    TerrainSpot spot;
    spot.place = stands - world.corner;
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
        float3 facing;
        float sunk, grain, shown;
        bool cubes = blockCubes(world, input.lod, stands, normal, spot, facing, sunk, grain, shown);
        // A face that joins those beside it reads its pictures' tile for where on it this is.
        spot = blockJoined(world, input.lod, spot, normal, uint(input.uv.y + 0.5));
        layer = blockLayer(world, input.lod, spot, normal, input.tangent);
        // A face whose material is cut out, as leaves are, is not there where its picture is clear.
        float cutoff = buffers()->materials[world.surfaces[min(input.lod, world.surfaceCount - 1)].material].alphaCutoff;
        if (cutoff > 0.0 && layer.base.a < cutoff)
        {
            discard;
        }
        if (cubes)
        {
            // A small cube's own face in place of the picture's bumps, turned only a little from the face it is
            // cut into: lit as a wall would be, a side away from the sun is black beside the face, where light
            // finds its way into so shallow a pit. Its shade says the rest: darker the deeper a top lies, a side
            // darker than the face, least towards the sun, and each small cube a shade of its own.
            layer.normal = lerp(layer.normal, normalize(lerp(normal, facing, 0.25)), shown);
            bool side = dot(facing, normal) < 0.5;
            float lit = side ? lerp(0.55, 0.88, saturate(dot(facing, -push.frame->sunDirection) * 0.5 + 0.5)) : 1.0;
            layer.base.rgb *= lerp(1.0, (1.0 - 0.18 * sunk) * lit * (0.94 + 0.12 * grain), shown);
        }
    }
    base = layer.base;
    base.rgb *= shade;
    metallic = layer.metallic;
    roughness = layer.roughness;
    normal = normalizeOr(layer.normal, normal);
    return true;
}

struct BlockShadowOut
{
    float4 position : SV_Position;
    // Where it is from its world's corner, and the way its face looks, for its surface's picture.
    float3 place : TEXCOORD0;
    float3 normal : NORMAL;
    nointerpolation uint surface : SURFACE;
    nointerpolation uint world : WORLD;
};

// A vertex in a sun cascade's view (push.list), placed as the camera's view places it.
[shader("vertex")]
BlockShadowOut blockShadowVertex(uint vertexId : SV_VertexID)
{
    BlockPoint b = blockPoint(vertexId);
    BlockWorld world = buffers()->blockWorlds[push.level - 1];
    // Its picture stays on its face as the face sways.
    float3 stands = b.world;
    b.world += blockSway(push.frame, world, b, push.frame->time);
    BlockShadowOut o;
    o.position = transform(shadowViewProj(push.frame, push.list, 0), float4(b.world, 1.0));
    o.place = stands - world.corner;
    o.normal = b.normal;
    // Over its surface, which of the faces round it a face that joins is joined to.
    o.surface = b.surface | (b.joins << 24);
    o.world = push.level - 1;
    return o;
}

// A face whose material is cut out, as leaves are, casts its picture's shadow: the sun passes where it is clear.
[shader("fragment")]
void blockShadowFragment(BlockShadowOut input)
{
    BlockWorld world = buffers()->blockWorlds[input.world];
    if (world.surfaceCount == 0 || (world.flags & BLOCKS_SMOOTH) != 0)
    {
        return;
    }
    uint surface = input.surface & 0xFF;
    float cutoff = buffers()->materials[world.surfaces[min(surface, world.surfaceCount - 1)].material].alphaCutoff;
    if (cutoff <= 0.0)
    {
        return;
    }
    TerrainSpot spot;
    spot.place = input.place;
    spot.dx = ddx(spot.place);
    spot.dy = ddy(spot.place);
    float3 normal = normalize(input.normal);
    float4 tangent = float4(normalizeOr(float3(normal.y, -normal.x, 0.0), float3(1.0, 0.0, 0.0)), 1.0);
    spot = blockJoined(world, surface, spot, normal, input.surface >> 24);
    if (blockLayer(world, surface, spot, normal, tangent).base.a < cutoff)
    {
        discard;
    }
}
