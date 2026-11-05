// GPU culling. One thread per instance fills an indirect draw list:
//
// - Early (LIST_EARLY): what was visible last frame and is in the frustum.
// - Late (LIST_LATE): everything in the frustum that the depth pyramid,
//   built from the early pass, does not hide, minus what early already drew.
//   It also records visibility for next frame's early pass.
// - Shadows (LIST_SHADOW + cascade): casters inside each cascade.
// - Local shadows (LIST_LOCAL_SHADOW + light): casters within each local
//   light's reach that casts a shadow, drawn into each of its views.
//
// Each draw also picks the coarsest LOD whose error stays under a pixel. When
// an instance's LOD changes, both LODs are drawn for a moment with
// complementary dither patterns, so the switch fades instead of popping.
//
// With meshlet culling, a draw is not written but its meshlets are queued,
// and the meshlet pass (meshletMain) after each list draws those that
// survive the same tests, for the passes to draw instead.

#include "culling.slang"

uint selectLod(Mesh mesh, float scale, float pixelsPerUnit, float threshold)
{
    uint lod = 0;
    for (uint i = 1; i < mesh.lodCount; i++)
    {
        if (mesh.lods[i].error * scale * pixelsPerUnit > threshold)
        {
            break;
        }
        lod = i;
    }
    return lod;
}

// The LOD an instance draws: the one its renderer forces; or where its
// renderer says each LOD ends, by the share of the screen's height it covers;
// or else the coarsest within the threshold, scaled by its renderer's bias.
uint instanceLod(Frame* frame, Instance instance, Mesh mesh, float3 center, float radius, float scale, float pixelsPerUnit, float threshold)
{
    uint last = max(mesh.lodCount, 1) - 1;
    uint forced = (instance.flags >> INSTANCE_LOD_SHIFT) & 0xFF;
    if (forced != 0)
    {
        return min(forced - 1, last);
    }
    if ((instance.flags & INSTANCE_LOD_SCREEN) != 0)
    {
        // From the main camera, so shadows draw the LOD the view does.
        float tanHalf = float(frame->viewport.y) / (2.0 * frame->lodScale);
        float share = radius / (max(length(center - frame->cameraPosition), frame->near) * tanHalf);
        float ends[8] = { instance.lodScreen0.x, instance.lodScreen0.y, instance.lodScreen0.z, instance.lodScreen0.w,
                          instance.lodScreen1.x, instance.lodScreen1.y, instance.lodScreen1.z, instance.lodScreen1.w };
        uint lod = 0;
        while (lod < last && share < ends[lod])
        {
            lod++;
        }
        return lod;
    }
    return selectLod(mesh, scale, pixelsPerUnit, threshold * exp2(instance.lodBias));
}

// Queues each of a draw's meshlets for the meshlet pass, and grows the
// pass's dispatch to a workgroup for every round of them. The CPU sizes the
// lists so they always fit; should they not, every entry the pass reads is
// still written, as a meshlet of some draw.
void queueMeshlets(Frame* frame, uint list, uint packed, MeshLod lod)
{
    uint first;
    InterlockedAdd(buffers()->drawCount[queuedCountIndex(list)], lod.meshletCount, first);
    uint end = min(first + lod.meshletCount, meshletListCapacity(list, frame->meshletCapacity));
    if (end <= first)
    {
        return;
    }
    uint ignored;
    InterlockedMax(buffers()->drawCount[drawCountIndex(list)], (end + MESHLET_ROUND - 1) / MESHLET_ROUND, ignored);
    uint base = meshletListBase(list, frame->meshletCapacity);
    for (uint i = first; i < end; i++)
    {
        buffers()->meshletWork[base + i] = uint2(packed, i - first);
    }
}

void emit(uint list, uint instance, Mesh mesh, uint lod, uint fade)
{
    uint packed = fade | (lod << DRAW_LOD_SHIFT) | instance;
    if ((push.frame->meshletFlags & MESHLET_CULLING) != 0)
    {
        queueMeshlets(push.frame, list, packed, mesh.lods[lod]);
        return;
    }
    uint slot;
    InterlockedAdd(buffers()->drawCount[drawCountIndex(list)], 1, slot);
    if (slot >= push.maxDraws)
    {
        return;
    }
    DrawCommand draw;
    draw.indexCount = mesh.lods[lod].indexCount;
    draw.instanceCount = 1;
    draw.firstIndex = mesh.lods[lod].indexOffset;
    draw.vertexOffset = 0;
    draw.firstInstance = 0;
    uint index = list * push.maxDraws + slot;
    buffers()->draws[index] = draw;
    buffers()->drawInstances[index] = packed;
}

void count(uint stat)
{
    uint ignored;
    InterlockedAdd(buffers()->stats[stat], 1, ignored);
}

// Per-instance LOD history: bits 0-3 the current LOD, 4-7 the previous one,
// 8-23 fade progress, 31 set once the instance has been seen. The history is
// kept by place in the list, so an Obj new to its place starts afresh rather
// than fading from the LOD another Obj was drawn at there.
uint advanceLod(Frame* frame, uint instance, uint selected, out uint previous, out float fade)
{
    uint state = buffers()->lodState[instance];
    uint current = state & 0xF;
    previous = (state >> 4) & 0xF;
    fade = float((state >> 8) & 0xFFFF) / 65535.0;
    bool fresh = (buffers()->instances[instance].flags & INSTANCE_NEW) != 0;
    if ((state >> 31) == 0 || fresh)
    {
        current = selected;
        previous = selected;
        fade = 1.0;
    }
    else if (selected != current)
    {
        previous = current;
        current = selected;
        fade = 0.0;
    }
    else
    {
        fade = min(fade + frame->deltaTime * frame->lodFadeSpeed, 1.0);
    }
    buffers()->lodState[instance] = (1u << 31) | (uint(fade * 65535.0) << 8) | (previous << 4) | current;
    return current;
}

void emitFading(Frame* frame, uint list, uint instance, Mesh mesh, uint selected)
{
    uint previous;
    float fade;
    uint lod = advanceLod(frame, instance, selected, previous, fade);
    count(STAT_LOD + lod);
    if (fade >= 1.0 || previous == lod)
    {
        emit(list, instance, mesh, lod, 0);
        return;
    }
    emit(list, instance, mesh, lod, DRAW_FADING_IN);
    emit(list, instance, mesh, previous, DRAW_FADING_OUT);
    count(STAT_FADE_DRAWS);
}

// How many times the error a plant's shadow may show, against its own drawing.
static const float PLANT_SHADOW_COARSER = 4.0;

void cullShadow(Frame* frame, uint cascade, uint instance, Mesh mesh, float3 center, float radius, float scale)
{
    // A caster only a few of the cascade's texels across casts no shadow anyone would see there.
    static const float SMALLEST_TEXELS = 2.0;
    if (!insidePlanes(frame->cascadePlanes[cascade], center, radius) || radius < frame->cascadeTexel[cascade] * SMALLEST_TEXELS)
    {
        return;
    }
    // A small plant's shadow is short, as it stands on the ground, so it falls only on what is round it, which
    // shading finds in the cascade the plant stands in, or the next one over where it stands near their border.
    // Drawing grass into every cascade it overlaps, as other casters must be, would draw each tuft three or four
    // times. Only a Foliage's plants: a tree's ball of leaves is as small, but stands metres up, and its shadow
    // falls metres away, in a cascade it was not drawn into, so it came and went as the camera turned.
    static const float SMALL = 1.0;
    static const float SHADOW_REACH = 4.0;
    Instance drawn = buffers()->instances[instance];
    if (radius < SMALL && drawn.fade.y > 0.0)
    {
        float depth = dot(center - frame->cameraPosition, frame->cameraForward);
        float nearest = cascade == 0 ? 0.0 : frame->cascadeFar[cascade - 1];
        float reach = radius * SHADOW_REACH;
        if (depth + reach < nearest || depth - reach > frame->cascadeFar[cascade])
        {
            return;
        }
    }
    // A plant's shadow is soft and broken up by its leaves, so a simpler LOD casts it as well: plants of a
    // Foliage, and anything drawn with a foliage material.
    bool plant = drawn.fade.y > 0.0 || (buffers()->materials[drawn.material].flags & MATERIAL_FOLIAGE) != 0;
    float threshold = frame->lodThreshold * (plant ? PLANT_SHADOW_COARSER : 1.0);
    uint lod = instanceLod(frame, drawn, mesh, center, radius, scale, 1.0 / frame->cascadeTexel[cascade], threshold);
    emit(LIST_SHADOW + cascade, instance, mesh, lod, 0);
    count(STAT_SHADOW_DRAWS);
}

// Casters within a local light's shadow, at the LOD a view of its atlas
// would pick where they stand.
void cullLocalShadow(Frame* frame, uint light, uint instance, Mesh mesh, float3 center, float radius, float scale)
{
    if (!insideLocalShadow(frame, light, center, radius))
    {
        return;
    }
    Instance drawn = buffers()->instances[instance];
    bool plant = drawn.fade.y > 0.0 || (buffers()->materials[drawn.material].flags & MATERIAL_FOLIAGE) != 0;
    float threshold = frame->lodThreshold * (plant ? PLANT_SHADOW_COARSER : 1.0);
    // A cube face's view is 90 degrees across, so its texels per metre at a distance are half its size over it.
    float away = max(distance(center, frame->localShadows.casters[light].xyz) - radius, 0.1);
    uint lod = instanceLod(frame, drawn, mesh, center, radius, scale, float(LOCAL_SHADOW_TILE) * 0.5 / away, threshold);
    emit(LIST_LOCAL_SHADOW + light, instance, mesh, lod, 0);
    count(STAT_SHADOW_DRAWS);
}

[shader("compute")]
[numthreads(64, 1, 1)]
void cullMain(uint3 id : SV_DispatchThreadID)
{
    Frame* frame = push.frame;
    if (id.x >= frame->instanceCount)
    {
        return;
    }

    Instance instance = buffers()->instances[id.x];
    Mesh mesh = buffers()->meshes[instance.mesh];
    float3 center = transform(instance.model, float4(mesh.center, 1.0)).xyz;
    float scale = max(length(instance.model.c0.xyz), max(length(instance.model.c1.xyz), length(instance.model.c2.xyz)));
    float radius = mesh.radius * scale;
    // A plant in the wind leans past its bounds by a share of its height (wind.slang), a little more in gusts.
    Material material = buffers()->materials[instance.material];
    if (frame->wind.strength > 0.0 && material.windBend > 0.0)
    {
        radius *= 1.0 + 0.1 * material.windBend * frame->wind.strength * (1.0 + frame->wind.gusts);
    }
    uint list = push.list + id.y;

    if (list >= LIST_SHADOW)
    {
        if ((instance.flags & INSTANCE_NO_SHADOW) != 0)
        {
            return;
        }
        if (list >= LIST_LOCAL_SHADOW)
        {
            cullLocalShadow(frame, list - LIST_LOCAL_SHADOW, id.x, mesh, center, radius, scale);
            return;
        }
        cullShadow(frame, list - LIST_SHADOW, id.x, mesh, center, radius, scale);
        return;
    }

    float distance = max(length(center - frame->cameraPosition) - radius, frame->near);
    uint lod = instanceLod(frame, instance, mesh, center, radius, scale, frame->lodScale / distance, frame->lodThreshold);
    bool inFrustum = insidePlanes(frame->planes, center, radius);

    if (list == LIST_EARLY)
    {
        if (inFrustum && buffers()->visibility[id.x] != 0)
        {
            emitFading(frame, LIST_EARLY, id.x, mesh, lod);
            count(STAT_DRAWN_EARLY);
        }
        return;
    }

    count(STAT_TESTED);
    bool visible = inFrustum && !occluded(frame, center, radius);
    if (!inFrustum)
    {
        count(STAT_FRUSTUM_CULLED);
    }
    else if (!visible)
    {
        count(STAT_OCCLUSION_CULLED);
    }
    if (visible && buffers()->visibility[id.x] == 0)
    {
        emitFading(frame, LIST_LATE, id.x, mesh, lod);
        count(STAT_DRAWN_LATE);
    }
    if (!visible)
    {
        // Hidden instances snap to the right LOD when they reappear.
        buffers()->lodState[id.x] = 0;
    }
    buffers()->visibility[id.x] = visible ? 1 : 0;
}

// The meshlet pass: a thread for each meshlet the cull queued, a round of
// them to a workgroup, whatever draws they belong to, so a LOD of a few
// meshlets leaves no lanes idle. Each meshlet that survives is drawn, and
// runs of them side by side, from the same draw, are drawn together.

// Each thread's meshlet in the round, by its draw and its place in its LOD,
// with ~0 for its draw when it did not survive.
groupshared uint roundDraws[MESHLET_ROUND];
groupshared uint roundMeshlets[MESHLET_ROUND];

// Writes one draw for each run of the round's surviving meshlets that lie
// side by side in one draw's LOD. A LOD's meshlets hold its indices in turn,
// so a run is one range of them: a draw whose meshlets all survive stays a
// draw or two rather than one a meshlet, which would cost the GPU's front
// end far more than culling saves. The meshlets debug view draws each
// meshlet on its own, to colour it by its first index.
void emitRuns(Frame* frame, uint list, uint thread, bool visible, uint2 entry, MeshLod lod)
{
    roundDraws[thread] = visible ? entry.x : ~0u;
    roundMeshlets[thread] = entry.y;
    GroupMemoryBarrierWithGroupSync();

    bool runs = (frame->meshletFlags & MESHLET_RUNS) != 0;
    uint before = max(thread, 1u) - 1;
    bool follows = runs && thread > 0 && roundDraws[before] == entry.x && roundMeshlets[before] + 1 == entry.y;
    bool starts = visible && !follows;
    uint run = 1;
    while (starts && runs && thread + run < MESHLET_ROUND && roundDraws[thread + run] == entry.x
           && roundMeshlets[thread + run] == entry.y + run)
    {
        run++;
    }

    uint lanes = WaveActiveCountBits(starts);
    uint first = 0;
    if (WaveIsFirstLane() && lanes > 0)
    {
        InterlockedAdd(buffers()->drawCount[meshletCountIndex(list)], lanes, first);
    }
    first = WaveReadLaneFirst(first);
    uint slot = first + WavePrefixCountBits(starts);
    if (!starts || slot >= meshletListCapacity(list, frame->meshletCapacity))
    {
        return;
    }
    uint begin = buffers()->meshlets[lod.meshletOffset + entry.y].firstIndex;
    uint next = entry.y + run;
    uint end = next < lod.meshletCount ? buffers()->meshlets[lod.meshletOffset + next].firstIndex : lod.indexCount;
    DrawCommand draw;
    draw.indexCount = end - begin;
    draw.instanceCount = 1;
    draw.firstIndex = lod.indexOffset + begin;
    draw.vertexOffset = 0;
    draw.firstInstance = 0;
    uint at = meshletListBase(list, frame->meshletCapacity) + slot;
    buffers()->meshletDraws[at] = draw;
    buffers()->meshletDrawInstances[at] = entry.x;
}

// Dispatched with the size the cull grew for list push.list.
[shader("compute")]
[numthreads(MESHLET_ROUND, 1, 1)]
void meshletMain(uint3 group : SV_GroupID, uint3 thread : SV_GroupThreadID)
{
    Frame* frame = push.frame;
    uint list = push.list;
    bool tested;
    uint2 entry;
    MeshLod lod;
    uint result = cullQueued(frame, list, group.x * MESHLET_ROUND + thread.x, tested, entry, lod);
    countMeshlets(list, tested, result);
    emitRuns(frame, list, thread.x, tested && result == MESHLET_VISIBLE, entry, lod);
}
