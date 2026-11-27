// Portals' openings (src/portals.rs). A view draws each opening it sees three ways:
//
// - Into its depth, as any surface (portalVertex alone), so nothing of its own world behind the opening is drawn.
// - As a mask for the view through it (portalMaskFragment): into that view's depth, the furthest depth where the
//   opening is the nearest thing in the pixel, so the far side fills exactly the pixels the opening shows in. Every
//   other pixel of that depth holds the nearest depth, which nothing passes.
// - As mist over it (portalMistFragment), whole while it is closed or its far side is not made, thinning as it opens.
//
// And once the views through them are drawn, portalDepthFragment makes the depth the passes after read: each pixel's
// own, or the far side's where an opening shows; and once the view's own water and glass are drawn,
// portalDepthNearerFragment lays theirs over that, where they came nearer.

#include "common.slang"

struct PortalOut
{
    float4 position : SV_Position;
    float2 uv : TEXCOORD0;
    nointerpolation uint quad : QUAD;
};

// Two triangles over an opening's four corners.
static const uint PORTAL_CORNERS[6] = { 0, 1, 2, 0, 2, 3 };

// The opening `push.level` names, its corners already where the view sees them.
[shader("vertex")]
PortalOut portalVertex(uint vertexId : SV_VertexID)
{
    PortalQuad quad = buffers()->portalQuads[push.level];
    uint corner = PORTAL_CORNERS[vertexId % 6];
    PortalOut o;
    o.position = quad.corners[corner];
    o.uv = float2(corner == 1 || corner == 2 ? 1.0 : 0.0, corner >= 2 ? 1.0 : 0.0);
    o.quad = push.level;
    return o;
}

// Whether the opening is the nearest thing in its pixel of the depth it was drawn into, push.texture. Drawn with
// depth clamped, what is nearer than the near plane lies at the nearest depth there.
bool portalShows(float4 position)
{
    float there = textures[NonUniformResourceIndex(push.texture)].Load(int3(int2(position.xy), 0)).r;
    float own = min(position.z, 1.0);
    return abs(there - own) <= max(own * 1e-4, 1e-7);
}

[shader("fragment")]
float portalMaskFragment(PortalOut input) : SV_Depth
{
    if (!portalShows(input.position))
    {
        discard;
    }
    return 0.0;
}

// A whole number's hash, 0 to 1: hashed in whole numbers, which every GPU agrees on.
float portalHash(int2 cell, int layer)
{
    uint h = uint(cell.x) * 0x9E3779B1u ^ uint(cell.y) * 0x85EBCA77u ^ uint(layer) * 0xC2B2AE3Du;
    h ^= h >> 15;
    h *= 0x2C1B3C6Du;
    h ^= h >> 12;
    h *= 0x297A2D39u;
    h ^= h >> 15;
    return float(h & 0xFFFFFFu) / 16777215.0;
}

// Smooth noise over `p`, 0 to 1, a cell a unit.
float portalNoise(float2 p, int layer)
{
    float2 cell = floor(p);
    float2 f = p - cell;
    float2 u = f * f * (3.0 - 2.0 * f);
    int2 c = int2(cell);
    float a = portalHash(c, layer);
    float b = portalHash(c + int2(1, 0), layer);
    float d = portalHash(c + int2(0, 1), layer);
    float e = portalHash(c + int2(1, 1), layer);
    return lerp(lerp(a, b, u.x), lerp(d, e, u.x), u.y);
}

// The mist: slow swirls of the Portal's colour, brighter towards its middle, lit by nothing, so it glows at night.
[shader("fragment")]
float4 portalMistFragment(PortalOut input) : SV_Target0
{
    if (!portalShows(input.position))
    {
        discard;
    }
    PortalQuad quad = buffers()->portalQuads[input.quad];
    float time = push.frame->time;
    float2 at = input.uv * float2(3.0, 4.5);
    float2 drift = float2(portalNoise(at * 0.7 + time * 0.11, 3), portalNoise(at * 0.7 - time * 0.09, 4)) - 0.5;
    float swirl = portalNoise(at + drift * 2.0 + float2(0.0, time * 0.15), 1) * 0.65
        + portalNoise(at * 2.3 - drift * 3.0 - float2(time * 0.07, 0.0), 2) * 0.35;
    float2 fromMiddle = input.uv * 2.0 - 1.0;
    float middle = saturate(1.0 - dot(fromMiddle, fromMiddle) * 0.45);
    float3 colour = quad.mist.rgb * (0.35 + 1.4 * swirl * swirl) * (0.55 + 0.45 * middle);
    return float4(colour, quad.mist.a);
}

struct FullOut
{
    float4 position : SV_Position;
};

[shader("vertex")]
FullOut portalDepthVertex(uint vertexId : SV_VertexID)
{
    FullOut o;
    float2 uv = float2((vertexId << 1) & 2, vertexId & 2);
    o.position = float4(uv * 2.0 - 1.0, 0.0, 1.0);
    return o;
}

// The view's own depth, push.textures.x, as the passes after its scene read it.
[shader("fragment")]
float portalDepthFragment(FullOut input) : SV_Target0
{
    return textures[NonUniformResourceIndex(push.textures.x)].Load(int3(int2(input.position.xy), 0)).r;
}

// The depth of a view through an opening, push.textures.x, over it: only where the opening showed, as everywhere
// else that depth holds the nearest there is.
[shader("fragment")]
float portalDepthOverFragment(FullOut input) : SV_Target0
{
    float far = textures[NonUniformResourceIndex(push.textures.x)].Load(int3(int2(input.position.xy), 0)).r;
    if (far >= 1.0)
    {
        discard;
    }
    return far;
}

// The view's depth now, push.textures.x, where it is nearer than it was before its water and glass were drawn,
// push.textures.y: their depth, over the far sides' too where they stand in front of an opening. Everywhere else the
// depth the passes after read stays as it is. Reversed depth: nearer is greater.
[shader("fragment")]
float portalDepthNearerFragment(FullOut input) : SV_Target0
{
    int3 at = int3(int2(input.position.xy), 0);
    float now = textures[NonUniformResourceIndex(push.textures.x)].Load(at).r;
    float then = textures[NonUniformResourceIndex(push.textures.y)].Load(at).r;
    if (now <= then)
    {
        discard;
    }
    return now;
}

// The light a Portal's far side sheds (src/portal_light.rs). One group reads the opening push.level at 16 by 16
// points across it, the first at its bottom left corner as its front sees it: each where the opening is the nearest
// thing in its pixel of the view's own depth (push.textures.x), from the picture as drawn (push.texture), and eased
// into the Portal's picture (push.output), which keeps what is not seen now; nothing kept where push.source is 1, as
// for a picture just made. The average of what is seen, and how much of the opening shows, go to the address in
// push.frame, at push.list.
groupshared float4 portalLightSums[256];

[shader("compute")]
[numthreads(16, 16, 1)]
void portalLightMain(uint3 id : SV_GroupThreadID, uint index : SV_GroupIndex)
{
    PortalQuad quad = buffers()->portalQuads[push.level];
    float2 uv = (float2(id.xy) + 0.5) / 16.0;
    // Between the corners as the view sees them, which keeps the perspective.
    float4 clip = lerp(lerp(quad.corners[0], quad.corners[1], uv.x), lerp(quad.corners[3], quad.corners[2], uv.x), uv.y);
    float4 seen = 0.0;
    if (clip.w > 1e-4)
    {
        float3 ndc = clip.xyz / clip.w;
        if (all(abs(ndc.xy) < 1.0) && ndc.z >= 0.0)
        {
            int2 at = int2((ndc.xy * 0.5 + 0.5) * float2(push.size));
            float there = textures[NonUniformResourceIndex(push.textures.x)].Load(int3(at, 0)).r;
            float own = min(ndc.z, 1.0);
            if (abs(there - own) <= max(own * 1e-2, 1e-6))
            {
                float3 colour = textures[NonUniformResourceIndex(push.texture)].Load(int3(at, 0)).rgb;
                if (all(isfinite(colour)))
                {
                    seen = float4(clamp(colour, 0.0, 256.0), 1.0);
                }
            }
        }
    }
    RWTexture2D<float4> picture = storageImagesRgba16f[push.output];
    float4 kept = push.source != 0 ? 0.0 : picture[id.xy];
    if (seen.w > 0.0)
    {
        kept = kept.w > 0.0 && all(isfinite(kept)) ? float4(lerp(kept.rgb, seen.rgb, 0.35), 1.0) : seen;
    }
    else if (!all(isfinite(kept)))
    {
        kept = 0.0;
    }
    picture[id.xy] = kept;
    portalLightSums[index] = seen;
    GroupMemoryBarrierWithGroupSync();
    for (uint stride = 128; stride > 0; stride >>= 1)
    {
        if (index < stride)
        {
            portalLightSums[index] += portalLightSums[index + stride];
        }
        GroupMemoryBarrierWithGroupSync();
    }
    if (index == 0)
    {
        float4 sum = portalLightSums[0];
        float4* averages = (float4*)push.frame;
        averages[push.list] = float4(sum.w > 0.0 ? sum.rgb / sum.w : 0.0, sum.w / 256.0);
    }
}
