// Filling the voxel GI's clipmap (gi.slang) each frame, in three compute
// passes after the opaque passes:
// - shift: last frame's voxels moved to this frame's origins, as the camera
//   moved, and faded a little, so what has gone fades out;
// - shadows: every few texels of the sun's shadow cascades, a voxel where
//   the sun sees a surface, so walls out of view still block and bounce;
// - view: every other pixel the camera sees, its lit colour into its voxel,
//   which is the light that surface sends on;
// - probes: the radiance cache's probes (gi.slang), an eighth of them each
//   frame and any the grid has just moved over, by tracing cones.

#include "atmosphere.slang"
#include "gi.slang"

// How much of last frame's voxels stays, each frame.
static const float GI_FADE = 0.9;
// What a surface the sun lights sends on, for those only the sun sees:
// a mid-grey surface, facing the sun half on.
static const float GI_SUNLIT = 0.08;

int2 giTexel(int3 voxel, uint level)
{
    return int2(voxel.x + voxel.z * GI_SIZE, voxel.y + int(level) * GI_SIZE);
}

// textures: x last frame's voxels.
[shader("compute")]
[numthreads(8, 8, 1)]
void giShiftMain(uint3 id : SV_DispatchThreadID)
{
    Frame* frame = push.frame;
    if (id.x >= uint(GI_SIZE * GI_SIZE) || id.y >= uint(GI_SIZE) * GI_LEVELS)
    {
        return;
    }
    uint level = id.y / uint(GI_SIZE);
    int3 voxel = int3(int(id.x) % GI_SIZE, int(id.y) % GI_SIZE, int(id.x) / GI_SIZE);
    int3 before = voxel + frame->gi.origin[level].xyz - frame->gi.previousOrigin[level].xyz;
    float4 kept = 0.0;
    bool inside = all(before >= 0) && all(before < GI_SIZE);
    if (frame->gi.reset == 0 && inside)
    {
        kept = textures[NonUniformResourceIndex(push.textures.x)].Load(int3(giTexel(before, level), 0)) * GI_FADE;
    }
    storageImagesRgba16f[push.output][int2(id.xy)] = kept;
}

// Marks the voxel at a point, in each grid holding it. `colour` is written
// unless something brighter is there already, from the camera.
void giMark(Frame* frame, float3 position, float3 colour, bool keepBrighter)
{
    for (uint level = 0; level < GI_LEVELS; level++)
    {
        float size = frame->gi.voxelSize * float(1u << level);
        int3 voxel = int3(floor(position / size)) - frame->gi.origin[level].xyz;
        if (any(voxel < 0) || any(voxel >= GI_SIZE))
        {
            continue;
        }
        int2 texel = giTexel(voxel, level);
        if (keepBrighter && storageImagesRgba16f[push.output][texel].a > 0.95)
        {
            continue;
        }
        storageImagesRgba16f[push.output][texel] = float4(colour, 1.0);
    }
}

// One thread for every fourth texel each way of the shadow atlas's four cascades.
[shader("compute")]
[numthreads(8, 8, 1)]
void giShadowsMain(uint3 id : SV_DispatchThreadID)
{
    Frame* frame = push.frame;
    if (frame->shadowTexture == NO_TEXTURE)
    {
        return;
    }
    uint2 atlasSize;
    textures[NonUniformResourceIndex(frame->shadowTexture)].GetDimensions(atlasSize.x, atlasSize.y);
    uint2 texel = id.xy * 4u;
    if (any(texel >= atlasSize))
    {
        return;
    }
    float depth = textures[NonUniformResourceIndex(frame->shadowTexture)].Load(int3(int2(texel), 0)).r;
    if (depth >= 0.999)
    {
        return;
    }
    uint2 half = atlasSize / 2u;
    uint cascade = (texel.x >= half.x ? 1u : 0u) + (texel.y >= half.y ? 2u : 0u);
    float2 uv = (float2(texel % half) + 0.5) / float2(half);
    float4 world = transform(frame->gi.cascadeInverse[cascade], float4(uv * 2.0 - 1.0, depth, 1.0));
    giMark(frame, world.xyz / world.w, frame->sunColor * GI_SUNLIT, true);
}

// One thread for every other pixel each way. textures: x the scene as the
// opaque passes drew it, y depth.
[shader("compute")]
[numthreads(8, 8, 1)]
void giViewMain(uint3 id : SV_DispatchThreadID)
{
    Frame* frame = push.frame;
    int2 pixel = int2(id.xy * 2u);
    if (any(pixel >= int2(frame->viewport)))
    {
        return;
    }
    float depth = textures[NonUniformResourceIndex(push.textures.y)].Load(int3(pixel, 0)).r;
    if (depth <= 0.0)
    {
        return;
    }
    float2 uv = (float2(pixel) + 0.5) / float2(frame->viewport);
    float4 world = transform(frame->invViewProj, float4(uv * 2.0 - 1.0, depth, 1.0));
    float3 colour = textures[NonUniformResourceIndex(push.textures.x)].Load(int3(pixel, 0)).rgb;
    // Highlights and emissive glare would light the room as lamps; a lit surface sends on at most this.
    float brightest = max(colour.r, max(colour.g, colour.b));
    colour *= min(1.0, 4.0 / max(brightest, 1e-4));
    giMark(frame, world.xyz / world.w, colour, false);
}

// Twelve directions, an icosahedron's corners, which cover the sphere evenly.
static const float3 PROBE_DIRECTIONS[12] = {
    float3(0.0, 0.525731, 0.850651), float3(0.0, -0.525731, 0.850651),
    float3(0.0, 0.525731, -0.850651), float3(0.0, -0.525731, -0.850651),
    float3(0.525731, 0.850651, 0.0), float3(-0.525731, 0.850651, 0.0),
    float3(0.525731, -0.850651, 0.0), float3(-0.525731, -0.850651, 0.0),
    float3(0.850651, 0.0, 0.525731), float3(-0.850651, 0.0, 0.525731),
    float3(0.850651, 0.0, -0.525731), float3(-0.850651, 0.0, -0.525731),
};
// Wide enough that twelve cones together cover the sphere.
static const float PROBE_CONE_TAN = 0.75;
// How much of a probe's last value a fresh one keeps, against flicker.
static const float PROBE_KEEP = 0.5;
// How bright each colour looks, for the direction light grows brighter in.
static const float3 PROBE_LUMA = float3(0.2126, 0.7152, 0.0722);

// The sky's light from straight up, the horizon and straight down, as
// shading has it.
void probeSky(Frame* frame, out float3 up, out float3 horizon, out float3 down)
{
    if (frame->lighting.sky == SKY_ATMOSPHERE)
    {
        float3 sun = sunThroughAtmosphere(frame);
        up = atmosphereAmbient(frame, 1.0, sun);
        down = atmosphereAmbient(frame, 0.0, sun);
    }
    else
    {
        up = lerp(frame->skyHorizon, frame->skyZenith, 0.5) * 0.35 * frame->ambient;
        down = frame->groundColor * 0.35 * frame->ambient;
    }
    horizon = (up + down) * 0.5;
}

// One thread a probe. textures: x last frame's probes.
[shader("compute")]
[numthreads(64, 1, 1)]
void giProbesMain(uint3 id : SV_DispatchThreadID)
{
    Frame* frame = push.frame;
    uint count = uint(GI_PROBES * GI_PROBES * GI_PROBES);
    if (id.x >= count)
    {
        return;
    }
    int index = int(id.x);
    int3 slot = int3(index % GI_PROBES, (index / GI_PROBES) % GI_PROBES, index / (GI_PROBES * GI_PROBES));
    int3 probe = slot + frame->gi.probeOrigin.xyz;
    int3 before = probe - frame->gi.probePreviousOrigin.xyz;
    bool known = frame->gi.reset == 0 && all(before >= 0) && all(before < GI_PROBES);
    float4 lastAverage = 0.0;
    float4 lastGrowth = 0.0;
    if (known)
    {
        Texture2D last = textures[NonUniformResourceIndex(push.textures.x)];
        lastAverage = last.Load(int3(giProbeTexel(before, 0), 0));
        lastGrowth = last.Load(int3(giProbeTexel(before, 1), 0));
    }
    // An eighth each frame, spread through the grid.
    bool due = (uint(index) + frame->frameIndex) % 8 == 0;
    if (known && !due)
    {
        storageImagesRgba16f[push.output][giProbeTexel(slot, 0)] = lastAverage;
        storageImagesRgba16f[push.output][giProbeTexel(slot, 1)] = lastGrowth;
        return;
    }

    float3 middle = (float3(probe) + 0.5) * frame->gi.probeSpacing;
    float3 skyUp;
    float3 skyHorizon;
    float3 skyDown;
    probeSky(frame, skyUp, skyHorizon, skyDown);
    float3 average = 0.0;
    float3 growth = 0.0;
    for (uint i = 0; i < 12; i++)
    {
        float3 direction = PROBE_DIRECTIONS[i];
        float4 cone = giCone(frame, middle, direction, PROBE_CONE_TAN);
        float3 light = cone.rgb * frame->gi.strength + (1.0 - cone.a) * giSky(direction, skyUp, skyHorizon, skyDown);
        average += light / 12.0;
        growth += dot(light, PROBE_LUMA) * direction / 12.0;
    }
    float brightness = max(dot(average, PROBE_LUMA), 1e-5);
    // How much of the probe's own voxel is open: a probe inside a wall sees only the wall.
    float open = 1.0 - giSample(frame, middle, 0).a;
    float4 fresh = float4(average, open);
    float4 freshGrowth = float4(growth / brightness, 0.0);
    storageImagesRgba16f[push.output][giProbeTexel(slot, 0)] = known ? lerp(fresh, lastAverage, PROBE_KEEP) : fresh;
    storageImagesRgba16f[push.output][giProbeTexel(slot, 1)] = known ? lerp(freshGrowth, lastGrowth, PROBE_KEEP) : freshGrowth;
}
