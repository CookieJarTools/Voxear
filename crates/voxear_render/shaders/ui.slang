// Voxear UI over the finished frame: rounded, bordered rectangles, text and
// icons from the glyph atlas in push.texture, soft shadows and lines. Each quad is six vertices; see
// src/ui_pass.rs. Colours arrive linear and premultiplied, and the output is
// sRGB, so blending happens in linear light.

#include "common.slang"

static const uint UI_GLYPH = 1;
static const uint UI_SHADOW = 2;
static const uint UI_LINE = 3;
static const uint UI_IMAGE = 4;
static const uint UI_DASHED = 1;
static const uint UI_FADES_DOWN = 1;
static const float3 UI_LUMA = float3(0.2126, 0.7152, 0.0722);
static const float2 UI_CORNERS[6] = {
    float2(0, 0), float2(1, 0), float2(0, 1),
    float2(0, 1), float2(1, 0), float2(1, 1),
};

struct UiOut
{
    float4 position : SV_Position;
    // Pixels from the quad's top left.
    float2 local : LOCAL;
    nointerpolation uint quad : QUAD;
};

[shader("vertex")]
UiOut uiVertex(uint vertexId : SV_VertexID)
{
    uint index = vertexId / 6;
    UiQuad q = buffers().uiQuads[index];
    float2 corner = UI_CORNERS[vertexId % 6];
    // Trimmed to its clip, so what lies outside is never rasterised; a quad
    // wholly outside collapses to nothing.
    float2 pixel = clamp(q.rect.xy + corner * q.rect.zw, q.clip.xy, q.clip.zw);
    UiOut o;
    o.position = float4(pixel / float2(push.size) * 2.0 - 1.0, 0.0, 1.0);
    o.local = pixel - q.rect.xy;
    o.quad = index;
    return o;
}

// Distance from a rounded box centred on the origin; negative inside.
float roundedBox(float2 p, float2 halfSize, float radius)
{
    float2 q = abs(p) - halfSize + radius;
    return length(max(q, 0.0)) + min(max(q.x, q.y), 0.0) - radius;
}

// Blending in linear light makes light text on a dark ground heavier than
// it was designed, and dark text on light thinner. This leans each back
// towards how the face looks in a design tool.
float textCoverage(float coverage, float4 color)
{
    float luma = dot(color.rgb / max(color.a, 1e-4), UI_LUMA);
    float light = pow(coverage, 1.45);
    float dark = 1.0 - pow(1.0 - coverage, 1.45);
    return lerp(dark, light, sqrt(saturate(luma)));
}

// The error function, to within about 5e-4, which a blur needs no closer.
float erfApprox(float x)
{
    float s = sign(x);
    float a = abs(x);
    float t = 1.0 + (0.278393 + (0.230389 + 0.078108 * a * a) * a) * a;
    t *= t;
    return s - s / (t * t);
}

// How much of a rounded box blurred by a Gaussian covers a pixel: the
// distance to the box, smoothed. Not exact at the corners, but there it is
// faint anyway.
float shadowCoverage(float2 pixel, float4 box, float radius, float sigma)
{
    float2 halfSize = box.zw * 0.5;
    float d = roundedBox(pixel - box.xy - halfSize, halfSize, min(radius, min(halfSize.x, halfSize.y)));
    return 0.5 - 0.5 * erfApprox(d / (max(sigma, 1e-3) * 1.41421356));
}

// Distance from a line segment.
float segment(float2 p, float2 a, float2 b)
{
    float2 ab = b - a;
    float t = saturate(dot(p - a, ab) / max(dot(ab, ab), 1e-6));
    return length(p - a - ab * t);
}

// A dashed border's pattern: on for half of each period, measured along the
// edge the pixel is nearest.
float dashes(float2 p, float2 halfSize, float width)
{
    float2 toEdge = halfSize - abs(p);
    float along = toEdge.y < toEdge.x ? p.x + halfSize.x : p.y + halfSize.y;
    float period = max(round(width * 6.0), 4.0);
    return step(fmod(along, period), period * 0.5);
}

[shader("fragment")]
float4 uiFragment(UiOut i) : SV_Target
{
    UiQuad q = buffers().uiQuads[i.quad];
    if (q.kind == UI_GLYPH)
    {
        float2 uv = lerp(q.uv.xy, q.uv.zw, i.local / q.rect.zw);
        float coverage = textures[NonUniformResourceIndex(push.texture)].SampleLevel(samplers[SAMPLER_LINEAR_CLAMP], uv, 0.0).r;
        return q.color * textCoverage(coverage, q.color);
    }
    float2 pixel = q.rect.xy + i.local;
    if (q.kind == UI_SHADOW)
    {
        float2 ownHalf = q.borderColor.zw * 0.5;
        float own = roundedBox(pixel - q.borderColor.xy - ownHalf, ownHalf, min(q.radius, min(ownHalf.x, ownHalf.y)));
        return q.color * shadowCoverage(pixel, q.uv, q.radius, q.border) * saturate(own + 0.5);
    }
    if (q.kind == UI_LINE)
    {
        float d = segment(pixel, q.uv.xy, q.uv.zw) - q.border * 0.5;
        return q.color * saturate(0.5 - d);
    }
    if (q.kind == UI_IMAGE)
    {
        // The texture is premultiplied already, as previews are; its box's corners are rounded.
        float2 uv = lerp(q.uv.xy, q.uv.zw, i.local / q.rect.zw);
        float4 image = textures[NonUniformResourceIndex(q.flags)].SampleLevel(samplers[SAMPLER_LINEAR_CLAMP], uv, 0.0);
        float2 imageHalf = q.rect.zw * 0.5;
        float inside = saturate(0.5 - roundedBox(i.local - imageHalf, imageHalf, min(q.radius, min(imageHalf.x, imageHalf.y))));
        return image * q.color * inside;
    }

    float2 halfSize = q.rect.zw * 0.5;
    float radius = min(q.radius, min(halfSize.x, halfSize.y));
    float2 p = i.local - halfSize;
    float d = roundedBox(p, halfSize, radius);
    // One pixel of antialiasing across each edge.
    float outer = saturate(0.5 - d);
    float inner = saturate(0.5 - (d + q.border));
    float edge = outer - inner;
    if ((q.flags & UI_DASHED) != 0)
    {
        edge *= dashes(p, halfSize, q.border);
    }
    return q.color * inner + q.borderColor * edge;
}
