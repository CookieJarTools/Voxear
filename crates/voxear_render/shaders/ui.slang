// Voxear UI over the finished frame: rounded, bordered rectangles, and text
// from the glyph atlas in push.texture. Each quad is six vertices; see
// src/ui_pass.rs. Colours arrive linear and premultiplied, and the output is
// sRGB, so blending happens in linear light.

#include "common.slang"

static const uint UI_GLYPH = 1;
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
    float2 pixel = q.rect.xy + corner * q.rect.zw;
    UiOut o;
    o.position = float4(pixel / float2(push.size) * 2.0 - 1.0, 0.0, 1.0);
    o.local = corner * q.rect.zw;
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

    float2 halfSize = q.rect.zw * 0.5;
    float radius = min(q.radius, min(halfSize.x, halfSize.y));
    float d = roundedBox(i.local - halfSize, halfSize, radius);
    // One pixel of antialiasing across each edge.
    float outer = saturate(0.5 - d);
    float inner = saturate(0.5 - (d + q.border));
    return q.color * inner + q.borderColor * (outer - inner);
}
