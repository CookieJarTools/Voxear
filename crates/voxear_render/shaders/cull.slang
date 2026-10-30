// GPU culling. One thread per instance fills an indirect draw list:
//
// - Early (LIST_EARLY): what was visible last frame and is in the frustum.
// - Late (LIST_LATE): everything in the frustum that the depth pyramid,
//   built from the early pass, does not hide, minus what early already drew.
//   It also records visibility for next frame's early pass.
// - Shadows (LIST_SHADOW + cascade): casters inside each cascade.
//
// Each draw also picks the coarsest LOD whose error stays under a pixel. When
// an instance's LOD changes, both LODs are drawn for a moment with
// complementary dither patterns, so the switch fades instead of popping.

#include "common.slang"

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

void emit(uint list, uint instance, Mesh mesh, uint lod, uint fade)
{
    uint slot;
    InterlockedAdd(buffers()->drawCount[list], 1, slot);
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
    buffers()->drawInstances[index] = fade | (lod << DRAW_LOD_SHIFT) | instance;
}

void count(uint stat)
{
    uint ignored;
    InterlockedAdd(buffers()->stats[stat], 1, ignored);
}

// Per-instance LOD history: bits 0-3 the current LOD, 4-7 the previous one,
// 8-23 fade progress, 31 set once the instance has been seen.
uint advanceLod(Frame* frame, uint instance, uint selected, out uint previous, out float fade)
{
    uint state = buffers()->lodState[instance];
    uint current = state & 0xF;
    previous = (state >> 4) & 0xF;
    fade = float((state >> 8) & 0xFFFF) / 65535.0;
    if ((state >> 31) == 0)
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

void cullShadow(Frame* frame, uint cascade, uint instance, Mesh mesh, float3 center, float radius, float scale)
{
    if (!insidePlanes(frame->cascadePlanes[cascade], center, radius))
    {
        return;
    }
    uint lod = selectLod(mesh, scale, 1.0 / frame->cascadeTexel[cascade], frame->lodThreshold);
    emit(LIST_SHADOW + cascade, instance, mesh, lod, 0);
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
    uint list = push.list + id.y;

    if (list >= LIST_SHADOW)
    {
        cullShadow(frame, list - LIST_SHADOW, id.x, mesh, center, radius, scale);
        return;
    }

    float distance = max(length(center - frame->cameraPosition) - radius, frame->near);
    uint lod = selectLod(mesh, scale, frame->lodScale / distance, frame->lodThreshold);
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
