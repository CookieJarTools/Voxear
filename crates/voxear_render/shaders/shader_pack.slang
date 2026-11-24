// What a shader pack's programs are written against: the frame as a pack sees it (PackFrame, src/shader_packs.rs's
// GpuPackFrame), the images of the frame a pass reads, and the pixel it is working out. A pack's program defines
//
//     float4 run(PassInput input)
//
// and returns the colour of its pixel: the frame's colour at its point, or its own target's. Voxear wraps each
// program in a file of its own that defines the pack's options and targets and which images its pass reads (see
// `wrapper` in src/shader_packs.rs), so a program reads only what its pass lists.
//
// Nothing here is the engine's own shaders: they change from version to version, and a pack's programs keep working.

[[vk::binding(0, 0)]] Texture2D textures[];
[[vk::binding(1, 0)]] SamplerState samplers[];

static const uint PACK_NO_TEXTURE = 0xFFFFFFFF;
static const uint PACK_LINEAR = 1;
static const uint PACK_NEAREST = 2;

#include "shader_pack_frame.slang"

// As the engine's own push constants are laid out (src/gpu.rs's Push).
struct PackPush
{
    PackFrame* frame;
    uint64_t engine;
    // The colour this pass reads.
    uint color;
    uint pass;
    uint list;
    uint output;
    uint2 size;
    uint level;
    uint source;
    uint4 textures;
};

[[vk::push_constant]] ConstantBuffer<PackPush> push;

PackFrame* frame()
{
    return push.frame;
}

// The pixel a pass works out: where it is from 0 to 1 across and down, in pixels, and how large the pass's picture is.
struct PassInput
{
    float2 uv;
    float2 pixel;
    float2 size;
};

struct PackVertex
{
    float4 position : SV_Position;
    float2 uv : TEXCOORD0;
};

[shader("vertex")]
PackVertex packVertex(uint vertexId : SV_VertexID)
{
    float2 ndc = float2((vertexId << 1) & 2, vertexId & 2) * 2.0 - 1.0;
    PackVertex o;
    o.position = float4(ndc, 0.0, 1.0);
    o.uv = ndc * 0.5 + 0.5;
    return o;
}

PassInput packInput(PackVertex v)
{
    PassInput input;
    input.uv = v.uv;
    input.pixel = v.position.xy;
    input.size = float2(push.size);
    return input;
}

// ---------------------------------------------------------------- Reading images

uint2 imageSize(uint texture)
{
    uint width;
    uint height;
    textures[NonUniformResourceIndex(texture)].GetDimensions(width, height);
    return uint2(width, height);
}

// An image at `uv`, filtered between its pixels; nothing where there is none.
float4 sampleImage(uint texture, float2 uv)
{
    if (texture == PACK_NO_TEXTURE)
    {
        return 0.0;
    }
    return textures[NonUniformResourceIndex(texture)].SampleLevel(samplers[PACK_LINEAR], uv, 0.0);
}

// An image's pixel, in its own pixels, kept to its edges.
float4 loadImage(uint texture, int2 pixel)
{
    if (texture == PACK_NO_TEXTURE)
    {
        return 0.0;
    }
    int2 size = int2(imageSize(texture));
    return textures[NonUniformResourceIndex(texture)].Load(int3(clamp(pixel, int2(0), size - 1), 0));
}

// The pixel of an image under `uv`, unfiltered.
float4 nearestImage(uint texture, float2 uv)
{
    if (texture == PACK_NO_TEXTURE)
    {
        return 0.0;
    }
    return loadImage(texture, int2(uv * float2(imageSize(texture))));
}

// One of the pack's own targets: this frame's, once a pass before has written it, and last frame's, for one it keeps.
struct PackTarget
{
    uint slot;

    float4 sample(float2 uv)
    {
        return sampleImage(frame()->targets[slot], uv);
    }

    float4 load(int2 pixel)
    {
        return loadImage(frame()->targets[slot], pixel);
    }

    float4 last(float2 uv)
    {
        return sampleImage(frame()->lastTargets[slot], uv);
    }

    uint2 size()
    {
        return imageSize(frame()->targets[slot]);
    }
};

float luminance(float3 color)
{
    return dot(color, float3(0.2126, 0.7152, 0.0722));
}

// Noise from 0 to 1 that is even across the picture and moves on each frame, for marching with fewer steps.
float frameNoise(float2 pixel)
{
    pixel += float(frame()->frameIndex % 64) * 5.588238;
    return frac(52.9829189 * frac(dot(pixel, float2(0.06711056, 0.00583715))));
}

// Where a point in the world shows in the picture, from 0 to 1 across and down, and whether it is in front.
float2 toPicture(float3 world, out bool inFront)
{
    float4 clip = packTransform(frame()->viewProjection, float4(world, 1.0));
    inFront = clip.w > 1e-5;
    return clip.xy / max(abs(clip.w), 1e-5) * 0.5 + 0.5;
}

// Whether the camera is orthographic: every ray runs along cameraForward, from its own point of the near plane.
bool isOrthographic()
{
    return frame()->orthoHalfHeight > 0.0;
}

// The way from the camera through the picture at `uv`: cameraForward everywhere for an orthographic camera.
float3 viewRay(float2 uv)
{
    if (isOrthographic())
    {
        return frame()->cameraForward;
    }
    float4 far = packTransform(frame()->inverseViewProjection, float4(uv * 2.0 - 1.0, 1e-6, 1.0));
    return normalize(far.xyz / far.w - frame()->cameraPosition);
}

// Where the camera's ray through `uv` starts: the camera, or for an orthographic camera its point of the near plane.
float3 viewRayStart(float2 uv)
{
    if (!isOrthographic())
    {
        return frame()->cameraPosition;
    }
    float4 near = packTransform(frame()->inverseViewProjection, float4(uv * 2.0 - 1.0, 1.0, 1.0));
    return near.xyz / near.w;
}

// How far in front of the camera, along cameraForward, something drawn at `depth` lies; very far for the sky.
float distanceOfDepth(float depth)
{
    if (depth <= 0.0)
    {
        return 1e7;
    }
    if (isOrthographic())
    {
        return frame()->near + (1.0 - saturate(depth)) * frame()->orthoDepth;
    }
    return frame()->near / depth;
}

// The sun's light where the camera is, through the air, and the sky's light from all round.
float3 sunLight()
{
    PackSkyLight* light = frame()->skyLight;
    return light != nullptr ? light->sun.rgb : frame()->sunLight;
}

float3 skyAround()
{
    PackSkyLight* light = frame()->skyLight;
    return light != nullptr ? light->sky.rgb : frame()->sunLight * 0.1;
}

#if PACK_READS_COLOR
// The frame's colour at the pass's point: light, before exposure, at every point but the last; at `final`, the
// picture as shown, from 0 to 1.
float4 sceneColor(float2 uv)
{
    return sampleImage(push.color, uv);
}

float4 sceneColorAt(int2 pixel)
{
    return loadImage(push.color, pixel);
}

uint2 colorSize()
{
    return imageSize(push.color);
}
#endif

#if PACK_READS_DEPTH
// The depth under `uv` as it is drawn: 1 at the near plane falling towards 0 far off, 0 for the sky.
float sceneDepth(float2 uv)
{
    return nearestImage(frame()->depthTexture, uv).r;
}

bool isSky(float2 uv)
{
    return sceneDepth(uv) <= 0.0;
}

// How far in front of the camera what `uv` shows is, in metres; very far for the sky.
float sceneDistance(float2 uv)
{
    return distanceOfDepth(sceneDepth(uv));
}

// Where what `uv` shows is, in the world.
float3 scenePosition(float2 uv)
{
    float depth = max(sceneDepth(uv), 1e-7);
    float4 point = packTransform(frame()->inverseViewProjection, float4(uv * 2.0 - 1.0, depth, 1.0));
    return point.xyz / point.w;
}
#endif

#if PACK_READS_NORMALS
float3 octahedronDecode(float2 e)
{
    float3 n = float3(e.x, 1.0 - abs(e.x) - abs(e.y), e.y);
    if (n.y < 0.0)
    {
        n.xz = (1.0 - abs(n.zx)) * select(n.xz >= 0.0, 1.0, -1.0);
    }
    return normalize(n);
}

// The way the surface `uv` shows faces, in the world; up for the sky.
float3 sceneNormal(float2 uv)
{
    float4 surface = nearestImage(frame()->surfaceTexture, uv);
    return any(surface.rg != 0.0) ? octahedronDecode(surface.rg * 2.0 - 1.0) : float3(0.0, 1.0, 0.0);
}

// How rough the surface is, from 0 for a mirror to 1.
float sceneRoughness(float2 uv)
{
    return nearestImage(frame()->surfaceTexture, uv).b;
}
#endif

#if PACK_READS_MOTION
// How far across the picture what `uv` shows has moved since last frame, from 0 to 1 across and down.
float2 sceneMotion(float2 uv)
{
    return nearestImage(frame()->motionTexture, uv).rg;
}

// Where what `uv` shows was in last frame's picture.
float2 lastUv(float2 uv)
{
    return uv - sceneMotion(uv);
}
#endif

#if PACK_READS_BLOOM
// The glow round bright light, as the engine's bloom works it out, as bright as what glows: added to the scene's
// light, a little of it, before a tonemap.
float3 sceneBloom(float2 uv)
{
    return sampleImage(push.textures.x, uv).rgb * frame()->bloomScale;
}

// Whether there is a glow: the Post Process's bloom is on.
bool bloomDrawn()
{
    return push.textures.x != PACK_NO_TEXTURE;
}
#endif

// The engine's tonemap curves, for a pass at the tonemap: light, exposed, to the picture as shown, from 0 to 1.
// A fit of the filmic curve films are graded to.
float3 tonemapFilmic(float3 x)
{
    return saturate((x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14));
}

// A curve that keeps colours as they are up to the highlights.
float3 tonemapNeutral(float3 color)
{
    static const float START = 0.76;
    static const float DESATURATION = 0.15;
    float x = min(color.r, min(color.g, color.b));
    color -= x < 0.08 ? x - 6.25 * x * x : 0.04;
    float peak = max(color.r, max(color.g, color.b));
    if (peak < START)
    {
        return saturate(color);
    }
    float d = 1.0 - START;
    float newPeak = 1.0 - d * d / (peak + d - START);
    color *= newPeak / peak;
    float g = 1.0 - 1.0 / (DESATURATION * (peak - newPeak) + 1.0);
    return saturate(lerp(color, float3(newPeak), g));
}

// The engine's default: bright colours go towards white as film's do, with `contrast` and `saturation` 1 as it has
// them.
float3 tonemapSoft(float3 c, float contrast = 1.0, float saturation = 1.0)
{
    float3 v = float3(0.842479062253094, 0.0423282422610123, 0.0423756549057051) * c.r
        + float3(0.0784335999999992, 0.878468636469772, 0.0784336) * c.g
        + float3(0.0792237451477643, 0.0791661274605434, 0.879142973793104) * c.b;
    static const float LOW = -12.47393;
    static const float HIGH = 4.026069;
    v = (clamp(log2(max(v, 1e-10)), LOW, HIGH) - LOW) / (HIGH - LOW);
    float3 v2 = v * v;
    float3 v4 = v2 * v2;
    v = 15.5 * v4 * v2 - 40.14 * v4 * v + 31.96 * v4 - 6.868 * v2 * v + 0.4298 * v2 + 0.1191 * v - 0.00232;
    v = pow(max(v, 0.0), contrast);
    float l = luminance(v);
    v = l + saturation * (v - l);
    float3 o = float3(1.19687900512017, -0.0528968517574562, -0.0529716355144438) * v.r
        + float3(-0.0980208811401368, 1.15190312990417, -0.0980434501171241) * v.g
        + float3(-0.0990297440797205, -0.0989611768448433, 1.15107367264116) * v.b;
    return pow(max(o, 0.0), 2.2);
}

#if PACK_READS_SURFACE_ID
// What the surface `uv` shows is: its kind in the top four bits (surfaceKind), and what of it below; 0 for the sky.
uint sceneSurfaceId(float2 uv)
{
    return asuint(nearestImage(frame()->surfaceIdTexture, uv).r);
}
#endif

// What kind of surface an id is: SURFACE_MODEL, SURFACE_BLOCK, SURFACE_TERRAIN, or 0 for the sky.
static const uint SURFACE_MODEL = PACK_SURFACE_MODEL;
static const uint SURFACE_BLOCK = PACK_SURFACE_BLOCK;
static const uint SURFACE_TERRAIN = PACK_SURFACE_TERRAIN;

uint surfaceKind(uint id)
{
    return id >> 28;
}

// A model's material's number, the same for every surface of that material.
uint surfaceMaterial(uint id)
{
    return id & 0x0FFFFFFF;
}

// A block face's look's number in its world, as BlockSurface.look has it.
uint surfaceBlockLook(uint id)
{
    return id & 0xFF;
}

// What a block face is, a bit each, as a surface program's BlockSurface.kind has it.
static const uint BLOCK_LEAVES = PACK_BLOCK_LEAVES;
static const uint BLOCK_SHEET = PACK_BLOCK_SHEET;
static const uint BLOCK_PART = PACK_BLOCK_PART;
static const uint BLOCK_CUT_OUT = PACK_BLOCK_CUT_OUT;
static const uint BLOCK_GLOWS = PACK_BLOCK_GLOWS;
static const uint BLOCK_SMOOTH = PACK_BLOCK_SMOOTH;
static const uint BLOCK_CUBES = PACK_BLOCK_CUBES;

// What a block face is: BLOCK_LEAVES and the rest, a bit each. Glowing is left out: the depth passes do not know it.
uint surfaceBlockKind(uint id)
{
    return (id >> 16) & 0xFFF;
}

#if PACK_READS_HISTORY
// Last frame's colour, as it was after TAA: light, before exposure.
float4 lastColor(float2 uv)
{
    return sampleImage(frame()->lastColorTexture, uv);
}
#endif

#if PACK_READS_BLOCK_LIGHT
// Whether the camera is in a world of blocks whose light is kept round it.
bool hasBlockLight()
{
    PackLightWindow* window = frame()->lightWindow;
    return window != nullptr && window->found != 0;
}

// The colour of the blocks' light, as bright as the world has it.
float3 blockGlow()
{
    PackLightWindow* window = frame()->lightWindow;
    return window != nullptr ? window->glow.rgb : 0.0;
}

// The sky light and block light at `position` in the world, each from 0 to 1, read between the eight blocks round it
// that let light in, so it runs smoothly from block to block and never takes the dark inside a wall. Open sky where
// no light is kept.
float2 lightAround(float3 position)
{
    PackLightWindow* window = frame()->lightWindow;
    if (window == nullptr || window->found == 0)
    {
        return float2(1.0, 0.0);
    }
    float3 at = (position - window->corner) / window->blockSize - 0.5;
    int3 low = int3(floor(at));
    if (any(low < int3(-1)) || any(low >= PACK_LIGHT_WINDOW))
    {
        return float2(1.0, 0.0);
    }
    float3 f = at - float3(low);
    float2 sum = 0.0;
    float weight = 0.0;
    for (uint corner = 0; corner < 8; corner++)
    {
        int3 step = int3(corner & 1, (corner >> 1) & 1, corner >> 2);
        int3 cell = low + step;
        uint light = 0xF0;
        if (all(cell >= int3(0)) && all(cell < PACK_LIGHT_WINDOW))
        {
            uint index = (uint(cell.y) * uint(PACK_LIGHT_WINDOW.z) + uint(cell.z)) * uint(PACK_LIGHT_WINDOW.x) + uint(cell.x);
            light = (frame()->lightBytes[index >> 2] >> ((index & 3) * 8)) & 0xFF;
        }
        if (light == PACK_LIGHT_SOLID)
        {
            continue;
        }
        float3 share = lerp(1.0 - f, f, float3(step));
        float w = share.x * share.y * share.z;
        sum += w * float2(float(light >> 4), float(light & 15)) / 15.0;
        weight += w;
    }
    return weight > 1e-4 ? sum / weight : float2(0.0, 0.0);
}
#endif

#if PACK_READS_SHADOW
[[vk::binding(3, 0)]] SamplerComparisonState packComparisonSamplers[];
static const uint PACK_COMPARISON_SHADOW = 0;

// Where a point falls in the sun's shadow pages: the page of the level asked for, or of the next coarser one drawn.
struct PackShadowPage
{
    bool found;
    // The page, its slot's first texel in its pool, the page's width and a texel's, in metres, and the pool.
    int2 page;
    float2 corner;
    float metres;
    float texel;
    uint texture;
    float poolSize;
};

float2 packPageUv(PackShadowPage page, float2 across)
{
    float border = page.texel * float(PACK_VSM_BORDER);
    float2 within = (across - (float2(page.page) * page.metres - border)) / (page.metres + 2.0 * border) * float(PACK_VSM_SLOT);
    return (page.corner + clamp(within, 1.0, float(PACK_VSM_SLOT) - 1.0)) / page.poolSize;
}

PackShadowPage packShadowPage(PackSunShadows* maps, float2 across, uint level)
{
    PackShadowPage found;
    found.found = false;
    found.page = 0;
    found.corner = 0.0;
    found.metres = 0.0;
    found.texel = 0.0;
    found.texture = maps->pool;
    found.poolSize = 1.0;
    for (uint k = level; k < PACK_VSM_LEVELS; k++)
    {
        float metres = maps->finestTexel * float(PACK_VSM_PAGE) * float(1u << k);
        int2 page = int2(floor(across / metres));
        int2 at = page - maps->origins[k].xy;
        if (any(at < 0) || any(at >= PACK_VSM_WINDOW))
        {
            continue;
        }
        uint entry = maps->table[k * uint(PACK_VSM_WINDOW * PACK_VSM_WINDOW) + uint(at.y * PACK_VSM_WINDOW + at.x)];
        if (entry == 0)
        {
            continue;
        }
        // Where something moves in it this frame, the copy with it drawn over.
        uint moving = entry >> 16;
        bool copied = moving != 0 && maps->movingPool != PACK_NO_TEXTURE;
        uint slot = copied ? moving - 1 : (entry & 0xFFFF) - 1;
        uint slots = copied ? PACK_VSM_MOVING_POOL_SLOTS : PACK_VSM_POOL_SLOTS;
        found.found = true;
        found.page = page;
        found.corner = float2(slot % slots, slot / slots) * float(PACK_VSM_SLOT);
        found.metres = metres;
        found.texel = metres / float(PACK_VSM_PAGE);
        found.texture = copied ? maps->movingPool : maps->pool;
        found.poolSize = float(PACK_VSM_SLOT * slots);
        return found;
    }
    return found;
}

// The level of the sun's shadow pages a point reads: the one whose texels are as wide as a pixel is there, or coarser
// for one whose texels are `reach` metres wide.
uint packShadowLevel(PackSunShadows* maps, float3 position, float reach)
{
    PackFrame* f = frame();
    float pixel = (isOrthographic() ? 1.0 : max(distance(position, f->cameraPosition), f->near)) / maps->lodScale;
    float level = log2(max(max(pixel, reach) / maps->finestTexel, 1.0)) + maps->bias;
    return uint(clamp(floor(level), 0.0, float(PACK_VSM_LEVELS - 1)));
}

uint packCascadeAt(PackSunShadows* maps, float3 position)
{
    float depth = dot(position - frame()->cameraPosition, frame()->cameraForward);
    uint cascade = 0;
    while (cascade < PACK_CASCADES && depth > maps->cascadeFar[cascade])
    {
        cascade++;
    }
    return cascade;
}

// Whether the sun's shadows are drawn at a point: a page of the sun's shadow pages holds it, or one of its cascades
// does. Where they are not, nothing in the sun's way is known, and sunVisibility says the sun reaches it.
bool sunShadowsReach(float3 position)
{
    PackSunShadows* maps = frame()->sunShadows;
    if (maps == nullptr)
    {
        return false;
    }
    if (maps->pool != PACK_NO_TEXTURE)
    {
        float2 across = float2(dot(position, maps->right), dot(position, maps->up));
        return packShadowPage(maps, across, packShadowLevel(maps, position, 0.0)).found;
    }
    return maps->atlas != PACK_NO_TEXTURE && packCascadeAt(maps, position) < PACK_CASCADES;
}

// How much of the sun reaches a point in the world, by the sun's shadow: 1 where nothing stands between it and the sun,
// 0 where something does, between on a shadow's edge. Anywhere, in the air as well as on a surface: a pass marching
// through the air takes it at each step for shafts of light. Read from the finest shadow there is round the point, or,
// with `reach`, from shadow texels about `reach` metres wide, cheaper and smoother for a march with long steps. 1 where
// the sun's shadows do not reach (sunShadowsReach). The clouds' shadow is not in it.
float sunVisibility(float3 position, float reach = 0.0)
{
    PackSunShadows* maps = frame()->sunShadows;
    if (maps == nullptr)
    {
        return 1.0;
    }
    if (maps->pool != PACK_NO_TEXTURE)
    {
        float2 across = float2(dot(position, maps->right), dot(position, maps->up));
        PackShadowPage page = packShadowPage(maps, across, packShadowLevel(maps, position, reach));
        if (!page.found)
        {
            return 1.0;
        }
        float depth = (dot(position, maps->direction) - maps->depthMin) / maps->depthRange;
        Texture2D pool = textures[NonUniformResourceIndex(page.texture)];
        return pool.SampleCmpLevelZero(packComparisonSamplers[PACK_COMPARISON_SHADOW], packPageUv(page, across), depth);
    }
    uint cascade = packCascadeAt(maps, position);
    if (maps->atlas == PACK_NO_TEXTURE || cascade >= PACK_CASCADES)
    {
        return 1.0;
    }
    float4 clip = packTransform(maps->cascadeViewProjection[cascade], float4(position, 1.0));
    float2 uv = clip.xy * 0.5 + 0.5;
    if (any(uv < 0.0) || any(uv > 1.0) || clip.z > 1.0)
    {
        return 1.0;
    }
    // The cascades' tiles, two by two across the atlas, each kept a texel and a half within its edges.
    float2 tile = float2(cascade & 1, cascade >> 1) * 0.5;
    float edge = 1.5 / float(2 * maps->cascadeTexels);
    float2 at = clamp(tile + uv * 0.5, tile + edge, tile + 0.5 - edge);
    return textures[NonUniformResourceIndex(maps->atlas)].SampleCmpLevelZero(
        packComparisonSamplers[PACK_COMPARISON_SHADOW], at, clip.z);
}
#endif
