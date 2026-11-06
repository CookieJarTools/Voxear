// Debug draw's lines over the finished frame (see src/debug_pass.rs and
// voxear_core::debug_draw). Each line is a quad as wide as it asks, with
// soft edges, six vertices pulled from buffers().debugLines. A line is
// dimmed where the scene is in front of it, unless it is drawn on top, by
// reading the scene's depth rather than testing against a depth attachment,
// since the view can be a rectangle of a larger output.
//
// push.textures: the scene's depth texture, the view's offset in the
// output (x, y), and the usual line width in pixels (as float bits).

#include "common.slang"

static const uint DEBUG_ON_TOP = 1;
static const uint DEBUG_IN_METRES = 2;
// How much of a line shows where the scene is in front of it.
static const float DEBUG_BEHIND = 0.3;
// Corners of a line's quad: along it (0 at `from`, 1 at `to`), and across.
static const float2 DEBUG_CORNERS[6] = {
    float2(0, -1), float2(1, -1), float2(0, 1),
    float2(0, 1), float2(1, -1), float2(1, 1),
};

struct DebugOut
{
    float4 position : SV_Position;
    float4 color : COLOR;
    // Pixels from the line's middle, across it, and how wide it is there.
    float across : ACROSS;
    float width : WIDTH;
    nointerpolation uint flags : FLAGS;
};

// Moves `a` along the line towards `b` until it is in front of the near
// plane, where w is the distance from the camera.
float4 inFront(float4 a, float4 b, float near)
{
    if (a.w >= near)
    {
        return a;
    }
    float t = (near - a.w) / max(b.w - a.w, 1e-6);
    return lerp(a, b, t);
}

[shader("vertex")]
DebugOut debugVertex(uint vertexId : SV_VertexID)
{
    DebugLine drawn = buffers()->debugLines[vertexId / 6];
    float2 corner = DEBUG_CORNERS[vertexId % 6];
    Frame* frame = push.frame;
    float near = frame->near;
    float4 a = transform(frame->unjitteredViewProj, float4(drawn.from, 1.0));
    float4 b = transform(frame->unjitteredViewProj, float4(drawn.to, 1.0));
    DebugOut o;
    o.color = drawn.color;
    o.flags = drawn.flags;
    o.across = 0.0;
    o.width = drawn.width > 0.0 ? drawn.width : asfloat(push.textures.w);
    // Wholly behind the camera: nothing to draw.
    if (a.w < near && b.w < near)
    {
        o.position = float4(0.0, 0.0, -1.0, 1.0);
        return o;
    }
    float4 start = inFront(a, b, near);
    float4 end = inFront(b, a, near);

    float2 halfView = float2(frame->viewport) * 0.5;
    float2 direction = end.xy / end.w * halfView - start.xy / start.w * halfView;
    direction = length(direction) > 1e-4 ? normalize(direction) : float2(1.0, 0.0);
    float2 normal = float2(-direction.y, direction.x);
    float4 p = lerp(start, end, corner.x);
    // Metres wide: as many pixels as that is at this end's distance, never under one. The camera's up, a metre
    // long, is this many clip units tall at a metre away.
    if ((drawn.flags & DEBUG_IN_METRES) != 0)
    {
        float3 up = float3(frame->view.c0.y, frame->view.c1.y, frame->view.c2.y);
        float tall = abs(transform(frame->unjitteredViewProj, float4(up, 0.0)).y);
        o.width = max(drawn.width * tall * halfView.y / max(p.w, near), 1.0);
    }
    // A pixel more than the width, for the soft edge.
    float reach = o.width * 0.5 + 1.0;
    p.xy += normal * corner.y * reach / halfView * p.w;
    o.position = p;
    o.across = corner.y * reach;
    return o;
}

[shader("fragment")]
float4 debugFragment(DebugOut i) : SV_Target
{
    float shown = 1.0;
    if ((i.flags & DEBUG_ON_TOP) == 0)
    {
        int2 pixel = int2(i.position.xy) - int2(push.textures.yz);
        float scene = textures[NonUniformResourceIndex(push.textures.x)].Load(int3(pixel, 0)).r;
        // Reverse-Z: nearer is larger. A little slack keeps lines lying on a
        // surface, and the TAA jitter, from flickering.
        if (i.position.z < scene * 0.999)
        {
            shown = DEBUG_BEHIND;
        }
    }
    float halfWidth = i.width * 0.5;
    float coverage = saturate(halfWidth + 0.5 - abs(i.across)) * shown;
    // Colours come as they look (sRGB), and the output blends in linear light.
    float3 c = i.color.rgb;
    float3 linearColor = select(c <= 0.04045, c / 12.92, pow((c + 0.055) / 1.055, 2.4));
    float alpha = i.color.a * coverage;
    return float4(linearColor * alpha, alpha);
}
