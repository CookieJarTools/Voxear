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

// The way from the camera through the picture at `uv`.
float3 viewRay(float2 uv)
{
    float4 far = packTransform(frame()->inverseViewProjection, float4(uv * 2.0 - 1.0, 1e-6, 1.0));
    return normalize(far.xyz / far.w - frame()->cameraPosition);
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
    float depth = sceneDepth(uv);
    return depth > 0.0 ? frame()->near / depth : 1e7;
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
