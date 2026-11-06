// Culling tests shared by the culling passes (cull.slang) and the task
// shader (meshlet_task.slang): spheres against planes and the depth pyramid,
// and meshlets against those and their normal cones. Then what the task
// shader hands the mesh shaders, and how they read it.

#pragma once

#include "common.slang"

// Whether a sphere lies where a local light's shadow reaches: within its
// range of the light, whichever of its views it falls in.
bool insideLocalShadow(Frame* frame, uint light, float3 center, float radius)
{
    float4 caster = frame->localShadows.casters[light];
    return distance(center, caster.xyz) < caster.w + radius;
}

bool insidePlanes(float4 planes[6], float3 center, float radius)
{
    // Five planes: the sixth is unused (infinite far plane, or open near side).
    for (int i = 0; i < 5; i++)
    {
        if (dot(planes[i].xyz, center) + planes[i].w < -radius)
        {
            return false;
        }
    }
    return true;
}

// True when the sphere's screen box is entirely behind the depth pyramid.
// With reverse-Z, larger depth is nearer, and the pyramid stores the minimum
// (farthest) depth of each region.
bool occluded(Frame* frame, float3 center, float radius)
{
    float2 low = 1e9;
    float2 high = -1e9;
    float nearest = 0.0;
    for (uint i = 0; i < 8; i++)
    {
        float3 corner = center + radius * float3((i & 1) != 0 ? 1.0 : -1.0, (i & 2) != 0 ? 1.0 : -1.0, (i & 4) != 0 ? 1.0 : -1.0);
        float4 clip = transform(frame->viewProj, float4(corner, 1.0));
        if (clip.w <= frame->near)
        {
            // Crosses the near plane: too close to judge, so draw it.
            return false;
        }
        float3 ndc = clip.xyz / clip.w;
        low = min(low, ndc.xy);
        high = max(high, ndc.xy);
        nearest = max(nearest, ndc.z);
    }
    float2 uvLow = saturate(low * 0.5 + 0.5);
    float2 uvHigh = saturate(high * 0.5 + 0.5);
    float2 pixels = (uvHigh - uvLow) * float2(frame->hizSize);
    // At this level the box spans at most one texel, so the 2x2 min footprint covers it.
    float level = min(ceil(log2(max(max(pixels.x, pixels.y), 1.0))), float(frame->hizMips - 1));
    float depth = textures[frame->hizTexture].SampleLevel(samplers[SAMPLER_MIN_REDUCTION], (uvLow + uvHigh) * 0.5, level).r;
    return nearest < depth;
}

// Adds how many lanes of the wave counted to a stat, with one atomic.
void countWave(uint stat, bool counted)
{
    uint lanes = WaveActiveCountBits(counted);
    if (WaveIsFirstLane() && lanes > 0)
    {
        uint ignored;
        InterlockedAdd(buffers()->stats[stat], lanes, ignored);
    }
}

// How many meshlets the cull queued in a list.
uint queuedMeshlets(Frame* frame, uint list)
{
    return min(buffers()->drawCount[queuedCountIndex(list)], meshletListCapacity(list, frame->meshletCapacity));
}

static const uint MESHLET_VISIBLE = 0;
static const uint MESHLET_OUTSIDE = 1;
static const uint MESHLET_BACK_FACING = 2;
static const uint MESHLET_OCCLUDED = 3;

// What testing one draw's meshlets needs, worked out once for the draw.
struct MeshletCuller
{
    Mat4 model;
    // The instance's largest scale, for the meshlets' radii, and how far its
    // material's custom shader may move a vertex.
    float scale;
    float grow;
    // -1 when the instance is mirrored, which turns its triangles' winding
    // round on screen, so the cone points the other way.
    float facing;
    // Whether back-facing meshlets go. Only where the rasterizer would cull
    // their triangles anyway, and only under a uniform scale, which keeps
    // a cone a cone.
    bool cones;
    uint list;
};

MeshletCuller meshletCuller(Frame* frame, Instance instance, uint list)
{
    float3 x = instance.model.c0.xyz;
    float3 y = instance.model.c1.xyz;
    float3 z = instance.model.c2.xyz;
    float3 lengths = float3(length(x), length(y), length(z));
    float scale = max(lengths.x, max(lengths.y, lengths.z));
    float tolerance = 1e-3 * scale * scale;
    bool uniform = (scale - min(lengths.x, min(lengths.y, lengths.z))) <= 1e-3 * scale
        && abs(dot(x, y)) <= tolerance && abs(dot(x, z)) <= tolerance && abs(dot(y, z)) <= tolerance;

    MeshletCuller culler;
    culler.model = instance.model;
    culler.scale = scale;
    culler.facing = dot(cross(x, y), z) < 0.0 ? -1.0 : 1.0;
    // An impostor's quad turns to the camera in its vertex shader, so it faces no way its cone could tell.
    bool turns = (instance.flags & INSTANCE_IMPOSTOR) != 0;
    // Nor can a cone tell which way a custom shader's moved vertices face.
    culler.grow = buffers()->materials[instance.material].maxOffset + instance.reach;
    bool moved = culler.grow > 0.0;
    culler.cones = list < LIST_SHADOW && (frame->meshletFlags & MESHLET_CONES) != 0 && uniform && !turns && !moved;
    culler.list = list;
    return culler;
}

// Whether a meshlet is drawn, or why not. Every list tests the frustum (a
// shadow cascade's own); the main view's lists also test the normal cone,
// and the late list the depth pyramid. The early list draws what was seen
// last frame without asking the pyramid, which is not built yet.
uint cullMeshlet(Frame* frame, MeshletCuller culler, Meshlet meshlet)
{
    float3 center = transform(culler.model, float4(meshlet.center, 1.0)).xyz;
    float radius = meshlet.radius * culler.scale + culler.grow;
    if (culler.list >= LIST_LOCAL_SHADOW)
    {
        return insideLocalShadow(frame, culler.list - LIST_LOCAL_SHADOW, center, radius) ? MESHLET_VISIBLE : MESHLET_OUTSIDE;
    }
    if (culler.list >= LIST_SHADOW)
    {
        return insidePlanes(frame->cascadePlanes[culler.list - LIST_SHADOW], center, radius) ? MESHLET_VISIBLE : MESHLET_OUTSIDE;
    }
    if (!insidePlanes(frame->planes, center, radius))
    {
        return MESHLET_OUTSIDE;
    }
    if (culler.cones && meshlet.coneCutoff < 1.0)
    {
        // The test without the cone's apex, from meshoptimizer: every
        // triangle faces away when the view lies inside the cone reversed.
        float3 axis = normalize(transform(culler.model, float4(meshlet.coneAxis, 0.0)).xyz) * culler.facing;
        float3 toCenter = center - frame->cameraPosition;
        if (dot(toCenter, axis) >= meshlet.coneCutoff * length(toCenter) + radius)
        {
            return MESHLET_BACK_FACING;
        }
    }
    if (culler.list == LIST_LATE && occluded(frame, center, radius))
    {
        return MESHLET_OCCLUDED;
    }
    return MESHLET_VISIBLE;
}

// Tests the meshlet the cull queued `work`th in a list, and says whether it
// is drawn, or why not, with its entry (its draw and its place in the draw's
// LOD) and the LOD. Past the last queued, `tested` is false.
uint cullQueued(Frame* frame, uint list, uint work, out bool tested, out uint2 entry, out MeshLod lod)
{
    tested = work < queuedMeshlets(frame, list);
    entry = tested ? buffers()->meshletWork[meshletListBase(list, frame->meshletCapacity) + work] : uint2(~0u, 0);
    lod = (MeshLod)0;
    if (!tested)
    {
        return MESHLET_OUTSIDE;
    }
    Instance instance = buffers()->instances[entry.x & DRAW_INSTANCE_MASK];
    lod = buffers()->meshes[instance.mesh].lods[(entry.x >> DRAW_LOD_SHIFT) & 0xF];
    Meshlet meshlet = buffers()->meshlets[lod.meshletOffset + entry.y];
    return cullMeshlet(frame, meshletCuller(frame, instance, list), meshlet);
}

// What the task shader hands its mesh shaders: each surviving meshlet's draw
// (as drawInstances) and its place in the draw's LOD.
struct MeshletPayload
{
    uint draws[MESHLET_ROUND];
    uint meshlets[MESHLET_ROUND];
};

// Counts a wave's meshlet results: what the main view tested, culled and
// drew, or what a shadow cascade or local light drew.
void countMeshlets(uint list, bool tested, uint result)
{
    if (list >= LIST_SHADOW)
    {
        countWave(STAT_MESHLETS_SHADOW_DRAWN, tested && result == MESHLET_VISIBLE);
        return;
    }
    countWave(STAT_MESHLETS_TESTED, tested);
    countWave(STAT_MESHLETS_FRUSTUM_CULLED, tested && result == MESHLET_OUTSIDE);
    countWave(STAT_MESHLETS_CONE_CULLED, tested && result == MESHLET_BACK_FACING);
    countWave(STAT_MESHLETS_OCCLUSION_CULLED, tested && result == MESHLET_OCCLUDED);
    countWave(STAT_MESHLETS_DRAWN, tested && result == MESHLET_VISIBLE);
}

// The meshlet a mesh shader's workgroup makes, with its draw's instance, LOD
// and role in a fade, and where its vertices and triangles start.
struct MeshletDrawn
{
    uint instance;
    uint lod;
    uint fade;
    Instance data;
    MeshLod meshLod;
    Meshlet meshlet;
    uint vertexBase;
    uint triangleBase;
};

MeshletDrawn meshletDrawn(MeshletPayload payload, uint group)
{
    uint packed = payload.draws[group];
    MeshletDrawn drawn;
    drawn.instance = packed & DRAW_INSTANCE_MASK;
    drawn.lod = (packed >> DRAW_LOD_SHIFT) & 0xF;
    drawn.fade = packed & (DRAW_FADING_IN | DRAW_FADING_OUT);
    drawn.data = buffers()->instances[drawn.instance];
    Mesh* mesh = &buffers()->meshes[drawn.data.mesh];
    drawn.meshLod = mesh->lods[drawn.lod];
    drawn.meshlet = buffers()->meshlets[drawn.meshLod.meshletOffset + payload.meshlets[group]];
    drawn.vertexBase = mesh->meshletVertexOffset + drawn.meshlet.vertexOffset;
    drawn.triangleBase = mesh->meshletTriangleOffset + drawn.meshlet.triangleOffset;
    return drawn;
}

uint3 meshletTriangle(MeshletDrawn drawn, uint index)
{
    uint corners = buffers()->meshletTriangles[drawn.triangleBase + index];
    return uint3(corners & 0xFF, (corners >> 8) & 0xFF, corners >> 16);
}
