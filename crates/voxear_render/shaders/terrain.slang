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
#include "lava.slang"

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

// How far tracks press into ground of these surfaces, as a share of their depth: each surface's own, those pressed
// `age` seconds ago filled in as far as it fills them.
float terrainGive(Terrain terrain, uint4 ids, float4 shares, float age)
{
    float give = 0.0;
    for (uint k = 0; k < 4; k++)
    {
        if (shares[k] <= 0.0)
        {
            continue;
        }
        TerrainSurface surface = terrain.surfaces[min(ids[k], terrain.surfaceCount - 1)];
        float kept = surface.fillsIn > 0.0 ? saturate(1.0 - age / surface.fillsIn) : 1.0;
        give += shares[k] * surface.tracks * kept;
    }
    return give;
}

// Where the eye's ray through `world` meets the floor of the tracks pressed `give` of their depth into the ground
// facing `normal`: how far from `world` along x and z, then how far down. Marched in steps down from the surface,
// then between the last two; nothing where the ground there is not pressed, or is seen almost edge on.
float3 terrainTrackFloor(Frame* frame, float3 world, float3 normal, float give)
{
    float sunk = trackAt(frame, world.xz).x * give;
    float3 ray = normalize(world - frame->cameraPosition);
    float down = -dot(ray, normal);
    if (sunk <= 1e-4 || down < 0.05)
    {
        return float3(0.0);
    }
    static const uint STEPS = 10;
    float deepest = min(max(sunk * 2.0, 0.05), 1.0);
    float lastDepth = 0.0;
    float lastSunk = sunk;
    for (uint i = 1; i <= STEPS; i++)
    {
        float depth = deepest * float(i) / float(STEPS);
        float2 along = ray.xz * (depth / down);
        float bottom = trackAt(frame, world.xz + along).x * give;
        if (bottom <= depth)
        {
            float above = lastSunk - lastDepth;
            float t = above / max(above - (bottom - depth), 1e-5);
            float met = lerp(lastDepth, depth, t);
            return float3(ray.xz * (met / down), met);
        }
        lastDepth = depth;
        lastSunk = bottom;
    }
    return float3(ray.xz * (deepest / down), deepest);
}

// The tracks at the pixel: its spot moved to where the eye sees the floor of the tracks, the normal bent by their
// slope there, and where they are deep, the terrain's first surface showing through those that gave. How deep the
// floor is, for the compacted look.
float terrainTracks(Terrain terrain, VertexOut input, inout TerrainSpot spot, inout float3 normal, inout uint4 ids, inout float4 shares)
{
    Frame* frame = push.frame;
    float age = frame->time - trackAt(frame, input.worldPosition.xz).y;
    float give = terrainGive(terrain, ids, shares, age);
    if (give <= 0.0)
    {
        return 0.0;
    }
    float3 met = terrainTrackFloor(frame, input.worldPosition, normal, give);
    if (met.z <= 0.0)
    {
        return 0.0;
    }
    spot.place += float3(met.x, -met.z, met.y);
    float2 at = input.worldPosition.xz + met.xy;
    float e = frame->tracks.texel;
    float gx = (trackAt(frame, at + float2(e, 0.0)).x - trackAt(frame, at - float2(e, 0.0)).x) * give / (2.0 * e);
    float gz = (trackAt(frame, at + float2(0.0, e)).x - trackAt(frame, at - float2(0.0, e)).x) * give / (2.0 * e);
    normal = normalize(normal + float3(gx, 0.0, gz));

    // Pressed deep, what gave thins to show the first surface under it.
    float through = 0.85 * smoothstep(0.04, 0.12, met.z);
    float moved = 0.0;
    for (uint k = 0; k < 4; k++)
    {
        TerrainSurface surface = terrain.surfaces[min(ids[k], terrain.surfaceCount - 1)];
        if (shares[k] > 0.0 && ids[k] != 0 && surface.tracks > 0.0)
        {
            float gone = shares[k] * through * saturate(surface.tracks);
            shares[k] -= gone;
            moved += gone;
        }
    }
    if (moved > 0.0)
    {
        // Into the first surface's place, or the least's.
        uint into = 0;
        for (uint k = 1; k < 4; k++)
        {
            into = shares[k] < shares[into] ? k : into;
        }
        for (uint k = 0; k < 4; k++)
        {
            into = ids[k] == 0 && shares[k] > 0.0 ? k : into;
        }
        shares[into] = ids[into] == 0 ? shares[into] + moved : moved;
        ids[into] = 0;
    }
    return met.z;
}

// Ground soaked through: darker, as wet soil and stone are, and glossier.
void terrainWet(inout TerrainLayer layer)
{
    layer.base.rgb *= 0.55;
    layer.roughness *= 0.35;
}

// How deep water would stand at `local`: how far the ground there lies below the lowest of the ground round it, on
// rings a metre, two and a half and five metres off, so a hollow fills from its middle up to the lowest point of its
// rim, and none stands on a slope that only dips. In metres.
float terrainHollow(Terrain terrain, float2 local)
{
    float here = terrainHeight(terrain, local);
    float deepest = 0.0;
    for (uint ring = 0; ring < 3; ring++)
    {
        float r = ring == 0 ? 1.0 : (ring == 1 ? 2.5 : 5.0);
        float rim = 1e9;
        for (uint i = 0; i < 12; i++)
        {
            float turn = (float(i) + 0.5 * float(ring)) * (PI / 6.0);
            rim = min(rim, terrainHeight(terrain, local + float2(cos(turn), sin(turn)) * r));
        }
        deepest = max(deepest, rim - here);
    }
    return deepest;
}

// Water on soaked ground: standing where the ground is level and lies `hollow` metres below the ground round it,
// its edge wandering, ruffled by the wind; running down slopes in rivulets along the ground's fall, faster the
// steeper. Mirror smooth, and darker under it.
void terrainPuddles(Frame* frame, float3 world, float3 ground, float hollow, inout TerrainLayer layer)
{
    terrainWet(layer);
    float edge = 0.005 + 0.025 * wetNoise(world.xz * 1.3);
    float standing = smoothstep(edge, edge + 0.035, hollow);
    float slope = 1.0 - ground.y;
    float2 down = ground.xz / max(length(ground.xz), 1e-4);
    float2 across = float2(-down.y, down.x);
    float speed = 0.3 + 4.0 * slope;
    float2 flow = float2(dot(world.xz, across) * 1.2, (dot(world.xz, down) - frame->time * speed) * 0.35);
    // Fewer and wider than a pixel: those thinner fade, rather than flicker.
    float fine = 1.0 - smoothstep(0.15, 0.4, fwidth(flow.x));
    float running = smoothstep(0.6, 0.7, wetNoise(flow)) * smoothstep(0.03, 0.08, slope) * (1.0 - smoothstep(0.45, 0.7, slope)) * fine;
    float water = max(standing, running);
    if (water <= 0.0)
    {
        return;
    }
    // Ripples: blown across standing water by the wind, carried down with running water.
    Wind wind = frame->wind;
    float2 blown = world.xz * 3.0 - wind.direction * frame->time * (0.5 + wind.strength);
    float2 rippled = standing >= running ? blown : float2(flow.x, flow.y * 6.0);
    float lift = standing >= running ? 0.04 + 0.1 * saturate(wind.strength) : 0.25;
    float2 bend = (float2(wetNoise(rippled), wetNoise(rippled + 31.7)) - 0.5) * lift;
    float3 surface = normalize(float3(bend.x, 1.0, bend.y));
    layer.base.rgb *= 1.0 - 0.45 * water;
    layer.roughness = lerp(layer.roughness, 0.02, water);
    layer.metallic *= 1.0 - water;
    layer.normal = normalize(lerp(layer.normal, surface, water));
}

// Lava's crust at `plate`, cracked into plates: how much of the pixel is crack, 0 to 1, and how hot it is there.
float2 terrainLavaCrust(Frame* frame, float2 plate)
{
    float2 scaled = plate / 1.5;
    scaled += (float2(lavaNoise(scaled * 0.7), lavaNoise(scaled * 0.7 + 17.0)) - 0.5) * 0.8;
    float2 cell = cellEdge(scaled);
    float width = 0.09 * (0.4 + 1.2 * lavaNoise(scaled * 1.3 + 5.0));
    float crack = 1.0 - smoothstep(width * 0.5, width, cell.x);
    // Hottest in the middle of a crack; the crust still dull red in patches.
    float molten = 1400.0 - 160.0 * saturate(cell.x / max(width, 1e-3)) - 60.0 * lavaNoise(plate * 1.7);
    float crust = 760.0 + 160.0 * lavaNoise(plate * 0.6 + frame->time * 0.03);
    return float2(crack, lerp(crust, molten, crack));
}

// Lava under a crust of the surface's own pictures, cracked into plates that ride down the ground's fall, faster
// the steeper and still where it is level, the molten rock in the cracks glowing by its heat. The plates are
// carried for twelve seconds at a time, two sets half a turn apart, each fading in and out, so the crust never
// stretches however long it flows. The light it gives.
float3 terrainLava(Frame* frame, float3 world, float3 ground, inout TerrainLayer layer)
{
    static const float CARRIED = 12.0;
    float2 fall = ground.xz * 2.5;
    float2 lava = float2(0.0);
    for (uint set = 0; set < 2; set++)
    {
        float turn = frac(frame->time / CARRIED + 0.5 * float(set));
        float weight = 1.0 - abs(2.0 * turn - 1.0);
        float2 crust = terrainLavaCrust(frame, world.xz + float2(31.0, 17.0) * float(set) - fall * turn * CARRIED);
        lava += float2(crust.x, crust.y) * weight;
    }
    float crack = saturate(lava.x);
    layer.base.rgb *= 1.0 - crack;
    layer.roughness = lerp(layer.roughness, 0.7, crack);
    return blackbody(lava.y) * 4.0;
}

// Grains of the ground that catch the sun: tiny flat faces a couple of centimetres apart, each turned its own way,
// a third of them bright when the eye looks along the sun's reflection off one; fading out far off, where they
// are smaller than a pixel. The light they give.
float3 terrainGlitter(Frame* frame, float3 world, float3 normal)
{
    float far = distance(frame->cameraPosition, world);
    if (far > 40.0)
    {
        return float3(0.0);
    }
    float2 grain = floor(world.xz * 45.0);
    float3 random = frac(sin(float3(dot(grain, float2(127.1, 311.7)), dot(grain, float2(269.5, 183.3)), dot(grain, float2(419.2, 371.9)))) * 43758.5453);
    if (random.z > 0.33)
    {
        return float3(0.0);
    }
    float3 facing = normalize(normal + float3(random.x * 2.0 - 1.0, 0.0, random.y * 2.0 - 1.0) * 0.35);
    float3 light = -frame->sunDirection;
    float3 halfway = normalize(normalize(frame->cameraPosition - world) + light);
    float spark = pow(saturate(dot(facing, halfway)), 900.0) * saturate(dot(normal, light));
    if (spark < 1e-4)
    {
        return float3(0.0);
    }
    bool atmosphere = frame->lighting.sky == SKY_ATMOSPHERE;
    float3 sun = atmosphere ? sunThroughAtmosphere(frame) : frame->sunColor;
    return sun * spark * 12.0 * sunShadow(frame, world, normal) * (1.0 - smoothstep(25.0, 40.0, far));
}

// What a surface's liquid and sparkle make of its pictures at the pixel, on ground facing `ground`; the light they
// give. `hollow` is how far the ground lies below the ground round it, worked out the first time it is wanted.
float3 terrainLiquid(Frame* frame, Terrain terrain, TerrainSurface surface, float2 local, float3 world, float3 ground, float pressed, inout float hollow, inout TerrainLayer layer)
{
    float3 light = float3(0.0);
    if ((surface.flags & TERRAIN_SURFACE_WET) != 0)
    {
        terrainWet(layer);
    }
    if ((surface.flags & TERRAIN_SURFACE_PUDDLES) != 0)
    {
        // Tracks are hollows too, so ruts fill with water.
        hollow = hollow < 0.0 ? terrainHollow(terrain, local) + pressed : hollow;
        terrainPuddles(frame, world, ground, hollow, layer);
    }
    if ((surface.flags & TERRAIN_SURFACE_LAVA) != 0)
    {
        light += terrainLava(frame, world, ground, layer);
    }
    if ((surface.flags & TERRAIN_SURFACE_GLITTER) != 0)
    {
        light += terrainGlitter(frame, world, layer.normal);
    }
    return light;
}

// The terrain's own look at the pixel, for shading.slang's fragment shaders (TERRAIN_SHADING): cuts away holes,
// then for a terrain with surfaces gives their colour, metallic, roughness and normal blended, and true.
bool terrainShading(VertexOut input, inout float4 base, inout float metallic, inout float roughness, out float3 normal, inout float3 emission)
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
    float pressed = (terrain.flags & TERRAIN_TRACKS) != 0 ? terrainTracks(terrain, input, spot, normal, ids, shares) : 0.0;

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
    float3 blendedLight = float3(0.0);
    float3 ground = normal;
    float hollow = -1.0;
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
        if ((surface.flags & ~TERRAIN_SURFACE_TRIPLANAR) != 0)
        {
            float3 at = input.worldPosition;
            blendedLight += terrainLiquid(push.frame, terrain, surface, local, at, ground, pressed, hollow, layer) * shares[k];
        }
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
    emission += blendedLight / used;
    // Ground pressed down is packed tighter: a little darker and smoother.
    float packed = saturate(pressed / 0.1);
    base.rgb *= 1.0 - 0.18 * packed;
    roughness *= 1.0 - 0.2 * packed;
    return true;
}
