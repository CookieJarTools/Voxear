// Terrain (src/terrain.rs): each patch a grid of TERRAIN_PATCH_QUADS quads
// each way, its vertices placed here from their index in the grid (one
// index buffer, the same for every patch, joins them into triangles), its
// heights read from the terrain's tiles. A vertex slides onto the next level's grid, every other
// one onto its neighbour, as it nears its level's far distance, so a level
// meets the next with no crack. The quads are split from their lowest
// corner to their highest, as the CPU's ground and physics split them.
//
// The ground is shaded by shading.slang's fragment shaders, as any mesh is:
// the terrain's anchor instance stands in for an instance's own. Where the
// terrain has surfaces, those shaders ask terrainShading (TERRAIN_SHADING)
// for its colour, roughness and normal in place of its material's: each
// sample keeps its four largest shares of the surfaces, which are blended
// between the four samples round the pixel, then by the surfaces' height
// pictures, so stones stand through mud where they meet. A triangle with a
// corner on a hole's sample is cut away, as physics cuts it.

#define TERRAIN_SHADING
#include "shading.slang"

#include "terrain_ground.slang"

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
    // Which terrain, for terrainShading (and a colour of its own in the meshlets view).
    o.cluster = p.terrain;
    o.currentClip = transform(frame->unjitteredViewProj, world);
    o.previousClip = transform(frame->prevViewProj, world);
    o.moved = 0.0;
    o.hidden = 0.0;
    return o;
}

struct TerrainShadowOut
{
    float4 position : SV_Position;
    float2 local : TEXCOORD0;
    nointerpolation uint terrain : TERRAIN;
};

// A vertex in a sun cascade's view (push.list), placed as the camera's view places it, so the ground's shadow
// falls from the very ground drawn.
[shader("vertex")]
TerrainShadowOut terrainShadowVertex(uint vertexId : SV_VertexID, uint instanceId : SV_InstanceID)
{
    TerrainPoint p = terrainPoint(push.frame, push.list, vertexId, instanceId);
    TerrainShadowOut o;
    o.position = transform(shadowViewProj(push.frame, push.list, 0), float4(p.world, 1.0));
    o.local = p.local;
    o.terrain = p.terrain;
    return o;
}

// Holes cast no shadow.
[shader("fragment")]
void terrainShadowFragment(TerrainShadowOut input)
{
    if (terrainCut(buffers()->terrains[input.terrain], input.local))
    {
        discard;
    }
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

// The terrain's own look at the pixel, for shading.slang's fragment shaders (TERRAIN_SHADING): cuts away holes,
// then for a terrain with surfaces gives their colour, metallic, roughness and normal blended, and true.
bool terrainShading(VertexOut input, inout float4 base, inout float metallic, inout float roughness, out float3 normal)
{
    normal = normalize(input.normal);
    Terrain terrain = buffers()->terrains[input.cluster];
    float2 local = input.worldPosition.xz - terrain.corner.xz;
    TerrainSpot spot;
    spot.place = float3(local.x, input.worldPosition.y, local.y);
    spot.dx = ddx(spot.place);
    spot.dy = ddy(spot.place);
    if (terrainCut(terrain, local))
    {
        discard;
    }
    if (terrain.surfaceCount == 0)
    {
        return false;
    }
    uint4 ids;
    float4 shares;
    terrainShares(terrain, local, ids, shares);

    // Blended by the surfaces' height pictures as sharply as they ask: each share raised by its picture's height,
    // and only what stands near the highest kept.
    uint count = 0;
    float4 heights = float4(0.0);
    float sharpness = 0.0;
    for (uint k = 0; k < 4; k++)
    {
        if (shares[k] > 0.0)
        {
            TerrainSurface surface = terrain.surfaces[min(ids[k], terrain.surfaceCount - 1)];
            heights[k] = terrainLayerHeight(surface, spot, normal);
            sharpness += surface.sharpness * shares[k];
            count++;
        }
    }
    if (count > 1 && sharpness > 0.0)
    {
        static const float DEPTH = 0.2;
        float4 raised = select(shares > 0.0, shares + heights, float4(-1.0));
        float top = max(max(raised.x, raised.y), max(raised.z, raised.w)) - DEPTH;
        float4 kept = max(raised - top, 0.0);
        kept /= max(kept.x + kept.y + kept.z + kept.w, 1e-5);
        shares = lerp(shares, kept, sharpness);
    }

    float4 blendedBase = float4(0.0);
    float3 blendedNormal = float3(0.0);
    float blendedMetallic = 0.0;
    float blendedRoughness = 0.0;
    float used = 0.0;
    for (uint k = 0; k < 4; k++)
    {
        if (shares[k] < 0.004)
        {
            continue;
        }
        TerrainSurface surface = terrain.surfaces[min(ids[k], terrain.surfaceCount - 1)];
        TerrainLayer layer = terrainLayerAt(surface, spot, normal, input.tangent);
        blendedBase += layer.base * shares[k];
        blendedNormal += layer.normal * shares[k];
        blendedMetallic += layer.metallic * shares[k];
        blendedRoughness += layer.roughness * shares[k];
        used += shares[k];
    }
    used = max(used, 1e-5);
    base = blendedBase / used;
    metallic = blendedMetallic / used;
    roughness = blendedRoughness / used;
    normal = normalizeOr(blendedNormal, normal);
    return true;
}
