// Depth-only rendering of shadow casters into one cascade of the atlas, or
// one view of a local light's.

#include "custom.slang"
#include "wind.slang"
#include "virtual_shadows.slang"

[shader("vertex")]
float4 shadowVertex(uint vertexId : SV_VertexID, uint drawId : SV_DrawIndex) : SV_Position
{
    uint lod;
    uint fade;
    Instance instance = buffers()->instances[drawInstance(drawId, lod, fade)];
    Mesh mesh = buffers()->meshes[instance.mesh];
    Vertex v = instanceVertex(instance, mesh, vertexId);
    float4 world = transform(instance.model, float4(v.position, 1.0));
    Material material = buffers()->materials[instance.material];
    float3 normal = normalize(instance.normal0.xyz * v.normal.x + instance.normal1.xyz * v.normal.y + instance.normal2.xyz * v.normal.z);
    float3 origin = instance.model.c3.xyz;
    // A virtual shadow page is kept from frame to frame, so what sways is drawn into it at rest.
    if (push.level < VSM_PAGE_VIEW)
    {
        world.xyz += windOffset(push.frame, material, origin, plantHeight(instance, mesh), length(instance.model.c1.xyz), world.xyz, normal, v.wind, push.frame->time);
    }
    // A custom shader's vertex function moves its shadow too.
    if (material.shader != STANDARD_SHADER)
    {
        float3 level = levelPosition(push.frame, world.xyz, 4096.0);
        VertexInput moved = { world.xyz, normal, v.uv, origin, push.frame->time, level };
        world.xyz += customVertex(instance.material, moved);
    }
    return transform(shadowViewProj(push.frame, push.list, push.level), world);
}

// A view of the kept atlas of what stands still (src/shadow_cache.rs) copied
// into the same place in the local shadow atlas, for what moves to be drawn
// over: one triangle over the view, for the view push.level names, where
// something moving is drawn for its light this frame, and none elsewhere.
[shader("vertex")]
float4 keptCopyVertex(uint vertexId : SV_VertexID) : SV_Position
{
    uint slot = push.level / LOCAL_SHADOW_SLOT_VIEWS;
    uint cached = push.frame->localShadows.cached;
    bool moving = ((cached >> (KEPT_MOVING_SHIFT + slot)) & 1) != 0
        || lampListDrawn(slot);
    if (!moving)
    {
        return float4(0.0, 0.0, 2.0, 1.0);
    }
    float2 corner = float2((vertexId << 1) & 2, vertexId & 2);
    return float4(corner * 2.0 - 1.0, 0.5, 1.0);
}

[shader("fragment")]
float keptCopyFragment(float4 position : SV_Position) : SV_Depth
{
    Texture2D kept = textures[push.frame->localShadows.cache];
    return kept.Load(int3(int2(position.xy), 0)).r;
}

// Asks for a page of a level at `across` in the sun's space, and the page beside it each way the point lies within a
// few texels of its edge, for filters that reach over.
void vsmAsk(Frame* frame, float2 across, uint level)
{
    float metres = vsmPageMetres(frame, level);
    float2 pages = across / metres;
    int2 page = int2(floor(pages));
    float2 within = (pages - float2(page)) * float(VSM_PAGE);
    int2 lowest = select(within < 4.0, page - 1, page);
    int2 highest = select(within > float(VSM_PAGE) - 4.0, page + 1, page);
    for (int y = lowest.y; y <= highest.y; y++)
    {
        for (int x = lowest.x; x <= highest.x; x++)
        {
            int2 at = int2(x, y) - frame->vsm.origins[level].xy;
            if (any(at < 0) || any(at >= VSM_WINDOW))
            {
                continue;
            }
            uint bit = level * uint(VSM_WINDOW * VSM_WINDOW) + uint(at.y * VSM_WINDOW + at.x);
            uint mask = 1u << (bit % 32);
            // Most pixels ask for a page others round them have asked for already: only the first writes.
            if ((frame->vsm.requests[bit / 32] & mask) != 0)
            {
                continue;
            }
            uint ignored;
            InterlockedOr(frame->vsm.requests[bit / 32], mask, ignored);
        }
    }
}

// The pages the view needs (src/vsm.rs): each texel of mip push.level, push.size large, of the depth pyramid in
// push.texture, the farthest depth of the few pixels under it, asks for the page holding it of the level whose texels
// are no wider than a pixel is there, and of the two coarser, which soft shadows read. The sky asks for none.
[shader("compute")]
[numthreads(8, 8, 1)]
void markMain(uint3 id : SV_DispatchThreadID)
{
    Frame* frame = push.frame;
    uint2 pixel = id.xy;
    if (any(pixel >= push.size))
    {
        return;
    }
    float depth = textures[push.texture].Load(int3(pixel, push.level)).r;
    if (depth <= 0.0)
    {
        return;
    }
    float2 ndc = (float2(pixel) + 0.5) / float2(push.size) * 2.0 - 1.0;
    float4 world = transform(frame->invViewProj, float4(ndc, depth, 1.0));
    float3 position = world.xyz / world.w;
    float2 across = vsmAcross(frame, position);
    // Far from the camera, as through a long lens, the finer levels' windows may not reach it: the first that does.
    uint level = vsmLevelFor(frame, position);
    while (level < VSM_LEVELS - 1 && !vsmWithin(frame, across, level))
    {
        level++;
    }
    for (uint k = level; k <= min(level + 2, VSM_LEVELS - 1); k++)
    {
        vsmAsk(frame, across, k);
    }
}

// Asks for page `page` of level `mip` of view `face` of lamp `slot`, unless a pixel round it has already.
void lampAsk(Frame* frame, uint slot, uint face, uint mip, int2 page)
{
    uint bit = lampPageIndex(slot, face, mip, page);
    uint mask = 1u << (bit % 32);
    if ((frame->lampVsm.requests[bit / 32] & mask) != 0)
    {
        return;
    }
    uint ignored;
    InterlockedOr(frame->lampVsm.requests[bit / 32], mask, ignored);
}

// The lamp pages the view needs (src/lamp_vsm.rs): each texel of mip push.level, push.size large, of the depth pyramid
// in push.texture, within a lamp's reach, asks for the page holding it of the view it falls in, of the level whose
// texels are no wider than a pixel is there, or no coarser than the atlas's, and of the two coarser, which soft
// shadows read.
[shader("compute")]
[numthreads(8, 8, 1)]
void markLampsMain(uint3 id : SV_DispatchThreadID)
{
    Frame* frame = push.frame;
    uint2 pixel = id.xy;
    if (any(pixel >= push.size))
    {
        return;
    }
    float depth = textures[push.texture].Load(int3(pixel, push.level)).r;
    if (depth <= 0.0)
    {
        return;
    }
    float2 ndc = (float2(pixel) + 0.5) / float2(push.size) * 2.0 - 1.0;
    float4 world = transform(frame->invViewProj, float4(ndc, depth, 1.0));
    float3 position = world.xyz / world.w;
    float pixelMetres = max(length(position - frame->cameraPosition), frame->near) / frame->lodScale;
    for (uint slot = 0; slot < frame->localShadows.count; slot++)
    {
        float4 caster = frame->localShadows.casters[slot];
        float4 light = frame->lampVsm.lights[slot];
        float3 offset = position - caster.xyz;
        float distance = length(offset);
        if (light.y < 1.0 || distance >= caster.w)
        {
            continue;
        }
        // A spot light's one view, or the face of the cube the point lies towards, as localShadowAt picks it.
        uint face = 0;
        if (light.y > 1.0)
        {
            float3 across = abs(offset);
            if (across.x >= across.y && across.x >= across.z)
            {
                face = offset.x >= 0.0 ? 0 : 1;
            }
            else if (across.y >= across.z)
            {
                face = offset.y >= 0.0 ? 2 : 3;
            }
            else
            {
                face = offset.z >= 0.0 ? 4 : 5;
            }
        }
        float4 clip = transform(frame->localShadows.viewProj[uint(light.x) + face], float4(position, 1.0));
        if (clip.w <= 0.0)
        {
            continue;
        }
        float2 at = clip.xy / clip.w;
        if (any(abs(at) > 1.0))
        {
            continue;
        }
        float2 uv = at * 0.5 + 0.5;
        float texel = max(2.0 * distance * light.z / float(LAMP_VSM_FINEST), 1e-6);
        float level = clamp(floor(log2(max(pixelMetres / texel, 1.0)) + frame->lampVsm.bias), 0.0, float(LAMP_VSM_ASKED));
        for (uint mip = uint(level); mip <= min(uint(level) + 2, LAMP_VSM_MIPS - 1); mip++)
        {
            int across = int((LAMP_VSM_FINEST / VSM_PAGE) >> mip);
            lampAsk(frame, slot, face, mip, clamp(int2(floor(uv * float(across))), 0, across - 1));
        }
    }
}
