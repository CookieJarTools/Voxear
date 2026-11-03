// Filling the voxel GI's clipmap (gi.slang) each frame, in compute passes
// round the voxelization (gi_voxelize.slang), after the opaque passes:
// - clear: the albedo, normal and emissive voxels emptied, for the meshes
//   to be drawn into again;
// - light: every voxel a mesh filled, lit as its surface is: by the sun,
//   where the shadow map says the sun reaches it, by lamps, its own glow, and by
//   the light the radiance cache found round it last frame, which bounces
//   it on again;
// - view: every other pixel the camera sees, its lit colour into its voxel
//   where that is brighter, which brings in the lamps;
// - downsample: each coarser grid, where the finer one covers it, made the
//   average of the finer voxels inside it;
// - probes: the radiance cache's probes (gi.slang), a sixteenth of them each
//   frame and any the grid has just moved over, by tracing cones.

#include "shading.slang"

int2 giTexel(int3 voxel, uint level)
{
    return int2(voxel.x + voxel.z * GI_SIZE, voxel.y + int(level) * GI_SIZE);
}

// One thread a voxel, of every grid. textures.xyz: the albedo, normal and
// emissive voxels as storage images.
[shader("compute")]
[numthreads(8, 8, 1)]
void giClearMain(uint3 id : SV_DispatchThreadID)
{
    if (id.x >= uint(GI_SIZE * GI_SIZE) || id.y >= uint(GI_SIZE) * GI_LEVELS)
    {
        return;
    }
    int2 texel = int2(id.xy);
    storageImagesRgba16f[push.textures.x][texel] = 0.0;
    storageImagesRgba16f[push.textures.y][texel] = 0.0;
    storageImagesRgba16f[push.textures.z][texel] = 0.0;
}

// What the level's lamps send on from a voxel of `size`: each lamp within
// reach, from whichever side of the voxel faces it, as a voxel can hold both
// faces of a thin wall. Lamps cast no shadows yet, so this is their light
// as shading has it.
float3 voxelLamps(Frame* frame, float3 middle, float3 normal, float3 albedo, float size)
{
    Surface s;
    s.view = normal;
    s.diffuse = albedo;
    s.f0 = 0.0;
    s.roughness = 1.0;
    float3 light = 0.0;
    for (uint i = 0; i < frame->lightCount; i++)
    {
        Light lamp = buffers()->lights[i];
        // Beyond its range and its shape's own size, a lamp gives nothing.
        if (distance(lamp.position, middle) > lamp.range + 2.0 * max(lamp.size.x, lamp.size.y) + size)
        {
            continue;
        }
        s.normal = dot(lamp.position - middle, normal) >= 0.0 ? normal : -normal;
        light += shadeLocalLight(s, lamp, middle + s.normal * size * 0.5);
    }
    return light;
}

// One thread a voxel, of every grid. textures.xyz: the albedo, normal and
// emissive voxels as storage images, w this frame's opacity. Writes this
// frame's voxels.
[shader("compute")]
[numthreads(8, 8, 1)]
void giLightMain(uint3 id : SV_DispatchThreadID)
{
    Frame* frame = push.frame;
    if (id.x >= uint(GI_SIZE * GI_SIZE) || id.y >= uint(GI_SIZE) * GI_LEVELS)
    {
        return;
    }
    int2 texel = int2(id.xy);
    float4 albedo = storageImagesRgba16f[push.textures.x][texel];
    if (albedo.a <= 0.0)
    {
        storageImagesRgba16f[push.output][texel] = 0.0;
        storageImagesRgba16f[push.textures.w][texel] = 0.0;
        return;
    }
    storageImagesRgba16f[push.textures.w][texel] = 1.0;
    float3 normal = storageImagesRgba16f[push.textures.y][texel].xyz;
    float3 emissive = storageImagesRgba16f[push.textures.z][texel].rgb;
    uint level = id.y / uint(GI_SIZE);
    int3 voxel = int3(int(id.x) % GI_SIZE, int(id.y) % GI_SIZE, int(id.x) / GI_SIZE);
    float size = giVoxelSize(frame, level);
    float3 middle = (float3(voxel + frame->gi.origin[level].xyz) + 0.5) * size;

    // A voxel can hold both faces of a thin wall, so light from either side
    // counts; whether the sun reaches it is asked just outside it, towards
    // the sun, which inside a thick wall is still in its shadow.
    float3 toSun = -frame->sunDirection;
    float facing = abs(dot(normal, toSun));
    bool atmosphere = frame->lighting.sky == SKY_ATMOSPHERE;
    float3 sunLight = atmosphere ? sunThroughAtmosphere(frame) : frame->sunColor;
    float shadow = sunShadowHard(frame, middle + toSun * size, toSun);
    float3 direct = albedo.rgb / PI * sunLight * facing * shadow;
    direct += voxelLamps(frame, middle, normal, albedo.rgb, size);

    // What reaches it from all round, as last frame's radiance cache found
    // it, so light bounces on a second time, and a third, over frames.
    float up = normal.y * 0.5 + 0.5;
    float3 sky = atmosphere ? atmosphereAmbient(frame, up, sunLight)
        : lerp(frame->groundColor, lerp(frame->skyHorizon, frame->skyZenith, 0.5), up) * 0.35 * frame->ambient;
    float3 around = frame->gi.probeTexture == NO_TEXTURE || frame->gi.reset != 0 ? sky : giAmbient(frame, middle, normal, sky);
    storageImagesRgba16f[push.output][texel] = float4(direct + albedo.rgb * around + emissive, 1.0);
}

// One thread a voxel of grid `push.level`, from 1. Where the finer grid
// covers it, made from the eight finer voxels inside it: the average of
// their light, and how much they block seen along each axis, where a column
// of two blocks if either does (textures.x, the opacity). A wall stays
// solid seen face on, as a ray meets it, while a cone passing along a floor
// is not lost in the coarse voxel round it. Grids are done finest first.
[shader("compute")]
[numthreads(8, 8, 1)]
void giDownsampleMain(uint3 id : SV_DispatchThreadID)
{
    Frame* frame = push.frame;
    if (id.x >= uint(GI_SIZE * GI_SIZE) || id.y >= uint(GI_SIZE))
    {
        return;
    }
    uint level = push.level;
    int3 voxel = int3(int(id.x) % GI_SIZE, int(id.y), int(id.x) / GI_SIZE);
    int3 child = (voxel + frame->gi.origin[level].xyz) * 2 - frame->gi.origin[level - 1].xyz;
    if (any(child < 0) || any(child + 1 >= GI_SIZE))
    {
        return;
    }
    float4 total = 0.0;
    float3 children[8];
    for (uint corner = 0; corner < 8; corner++)
    {
        int3 offset = int3(corner & 1, (corner >> 1) & 1, (corner >> 2) & 1);
        int2 texel = giTexel(child + offset, level - 1);
        total += storageImagesRgba16f[push.output][texel];
        children[corner] = storageImagesRgba16f[push.textures.x][texel].xyz;
    }
    // Corners differing by 1, 2 and 4 lie along X, Y and Z; each axis's columns are seen along that axis.
    float3 blocks = 0.0;
    for (uint corner = 0; corner < 8; corner++)
    {
        [unroll]
        for (uint axis = 0; axis < 3; axis++)
        {
            uint step = 1u << axis;
            if ((corner & step) == 0)
            {
                float near = children[corner][axis];
                float far = children[corner | step][axis];
                blocks[axis] += (1.0 - (1.0 - near) * (1.0 - far)) / 4.0;
            }
        }
    }
    int2 texel = giTexel(voxel, level);
    storageImagesRgba16f[push.output][texel] = total / 8.0;
    storageImagesRgba16f[push.textures.x][texel] = float4(blocks, 0.0);
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
    // The voxels lit from the meshes know only the sun; what the camera sees brings the lamps too.
    float3 position = world.xyz / world.w;
    for (uint level = 0; level < GI_LEVELS; level++)
    {
        int3 voxel = int3(floor(position / giVoxelSize(frame, level))) - frame->gi.origin[level].xyz;
        if (any(voxel < 0) || any(voxel >= GI_SIZE))
        {
            continue;
        }
        int2 texel = giTexel(voxel, level);
        float4 lit = storageImagesRgba16f[push.output][texel];
        storageImagesRgba16f[push.output][texel] = float4(max(lit.rgb, colour), 1.0);
        storageImagesRgba16f[push.textures.z][texel] = 1.0;
    }
}

// How many cones each probe traces, spread evenly over the sphere, and
// how wide each is: enough together to cover it, and narrow enough that a
// cone aimed at a wall does not also take in the floor beside the probe.
static const uint PROBE_CONES = 32;
static const float PROBE_CONE_TAN = 0.4;

// The cones' directions, on a Fibonacci spiral round the sphere.
float3 probeDirection(uint index)
{
    float y = 1.0 - (float(index) + 0.5) * 2.0 / float(PROBE_CONES);
    float ring = sqrt(1.0 - y * y);
    float angle = float(index) * 2.39996323;
    return float3(cos(angle) * ring, y, sin(angle) * ring);
}
// How much of a probe's last value a fresh one keeps, against flicker.
static const float PROBE_KEEP = 0.5;
// The ambient cube's faces, in the order the probe's texels hold them.
static const float3 PROBE_FACES[6] = {
    float3(1.0, 0.0, 0.0), float3(-1.0, 0.0, 0.0), float3(0.0, 1.0, 0.0),
    float3(0.0, -1.0, 0.0), float3(0.0, 0.0, 1.0), float3(0.0, 0.0, -1.0),
};

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
    // A sixteenth each frame, spread through the grid.
    bool due = (uint(index) + frame->frameIndex) % 16 == 0;
    Texture2D last = textures[NonUniformResourceIndex(push.textures.x)];
    if (known && !due)
    {
        for (int face = 0; face < GI_PROBE_FACES; face++)
        {
            storageImagesRgba16f[push.output][giProbeTexel(slot, face)] = last.Load(int3(giProbeTexel(before, face), 0));
        }
        return;
    }

    float3 middle = (float3(probe) + 0.5) * frame->gi.probeSpacing;
    float3 skyUp;
    float3 skyHorizon;
    float3 skyDown;
    probeSky(frame, skyUp, skyHorizon, skyDown);
    // Each face of the cube: the cones round it, weighted as light falls on a surface facing that way.
    float3 faces[6] = { 0.0, 0.0, 0.0, 0.0, 0.0, 0.0 };
    float weights[6] = { 0.0, 0.0, 0.0, 0.0, 0.0, 0.0 };
    for (uint i = 0; i < PROBE_CONES; i++)
    {
        float3 direction = probeDirection(i);
        float4 cone = giCone(frame, middle, direction, PROBE_CONE_TAN);
        float3 light = cone.rgb * frame->gi.strength + (1.0 - cone.a) * giSky(direction, skyUp, skyHorizon, skyDown);
        [unroll]
        for (uint face = 0; face < 6; face++)
        {
            float weight = max(dot(direction, PROBE_FACES[face]), 0.0);
            faces[face] += light * weight;
            weights[face] += weight;
        }
    }
    // How much of the probe's own voxel is open: a probe inside a wall sees only the wall.
    float open = 1.0 - giSample(frame, middle, 0).a;
    for (int face = 0; face < GI_PROBE_FACES; face++)
    {
        float4 fresh = float4(faces[face] / weights[face], open);
        float4 kept = known ? lerp(fresh, last.Load(int3(giProbeTexel(before, face), 0)), PROBE_KEEP) : fresh;
        storageImagesRgba16f[push.output][giProbeTexel(slot, face)] = kept;
    }
}
