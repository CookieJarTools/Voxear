// Voxear UI over the finished frame: rounded, bordered rectangles, text and
// icons from the glyph atlas in push.texture, soft shadows and lines. Each quad is six vertices; see
// src/ui_pass.rs. A shape with UI_BACKDROP shows the view blurred under its background: push.textures
// is the blurred view, where the view is in the output (x | y << 16), its size, and the blurred
// image's size, the same way packed. push.list is where the layer's area starts in the output
// (x | y << 16): the game's UI is laid out for its own area, as the Game view. push.level is the
// layer's first quad, since SV_VertexID counts from the draw's first vertex. Colours arrive
// linear and premultiplied, and the output is sRGB, so blending happens in linear light. A UI
// panel in the world is drawn the same way into a texture of its own (see uiPanelVertex below).

#include "common.slang"

static const uint UI_GLYPH = 1;
static const uint UI_SHADOW = 2;
static const uint UI_LINE = 3;
static const uint UI_IMAGE = 4;
static const uint UI_GRADIENT = 5;
static const uint UI_DASHED = 1;
static const uint UI_BACKDROP = 2;
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
    uint index = push.level + vertexId / 6;
    UiQuad q = buffers().uiQuads[index];
    float2 corner = UI_CORNERS[vertexId % 6];
    // Trimmed to its clip, so what lies outside is never rasterised; a quad
    // wholly outside collapses to nothing.
    float2 pixel = clamp(q.rect.xy + corner * q.rect.zw, q.clip.xy, q.clip.zw);
    float2 origin = float2(push.list & 0xFFFF, push.list >> 16);
    UiOut o;
    o.position = float4((pixel + origin) / float2(push.size) * 2.0 - 1.0, 0.0, 1.0);
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

// The view behind a pixel, blurred: the small blurred image, sampled four
// times around the point so its texels do not show as blocks.
float3 backdrop(float2 pixel)
{
    float2 origin = float2(push.textures.y & 0xFFFF, push.textures.y >> 16);
    float2 size = max(float2(push.textures.z & 0xFFFF, push.textures.z >> 16), 1.0);
    float2 blurred = max(float2(push.textures.w & 0xFFFF, push.textures.w >> 16), 1.0);
    float2 uv = (pixel - origin) / size;
    float2 texel = 0.5 / blurred;
    Texture2D image = textures[NonUniformResourceIndex(push.textures.x)];
    SamplerState clamped = samplers[SAMPLER_LINEAR_CLAMP];
    float3 sum = image.SampleLevel(clamped, uv + float2(-texel.x, -texel.y), 0.0).rgb
        + image.SampleLevel(clamped, uv + float2(texel.x, -texel.y), 0.0).rgb
        + image.SampleLevel(clamped, uv + float2(-texel.x, texel.y), 0.0).rgb
        + image.SampleLevel(clamped, uv + float2(texel.x, texel.y), 0.0).rgb;
    return sum * 0.25;
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
        // The texture is premultiplied already, as previews are; its box's corners are rounded. Its mip is
        // picked from how far the picture moves a pixel, worked out from the quad rather than from neighbouring
        // pixels, which may belong to another quad.
        float2 uv = lerp(q.uv.xy, q.uv.zw, i.local / q.rect.zw);
        float2 step = (q.uv.zw - q.uv.xy) / max(q.rect.zw, 1.0);
        float4 image = textures[NonUniformResourceIndex(q.flags)].SampleGrad(
            samplers[SAMPLER_LINEAR_CLAMP], uv, float2(step.x, 0.0), float2(0.0, step.y));
        float2 imageHalf = q.rect.zw * 0.5;
        float inside = saturate(0.5 - roundedBox(i.local - imageHalf, imageHalf, min(q.radius, min(imageHalf.x, imageHalf.y))));
        return image * q.color * inside;
    }
    if (q.kind == UI_GRADIENT)
    {
        // Blended in sRGB, as people see colour, then made linear and premultiplied as the rest are.
        float2 t = saturate(i.local / q.rect.zw);
        float4 across = lerp(q.color, q.borderColor, t.x);
        float4 c = (q.flags & UI_FADES_DOWN) != 0 ? lerp(across, q.uv, t.y) : across;
        float3 linearColor = select(c.rgb <= 0.04045, c.rgb / 12.92, pow((c.rgb + 0.055) / 1.055, 2.4));
        float2 boxHalf = q.rect.zw * 0.5;
        float inside = saturate(0.5 - roundedBox(i.local - boxHalf, boxHalf, min(q.radius, min(boxHalf.x, boxHalf.y))));
        return float4(linearColor * c.a, c.a) * inside;
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
    float4 fill = q.color;
    if ((q.flags & UI_BACKDROP) != 0 && push.textures.x != NO_TEXTURE)
    {
        // The background over what is behind, blurred: frosted glass, opaque.
        float2 origin = float2(push.list & 0xFFFF, push.list >> 16);
        fill = float4(backdrop(pixel + origin) * (1.0 - q.color.a) + q.color.rgb, 1.0);
    }
    return fill * inner + q.borderColor * edge;
}

// UI panels in the world (see src/ui_panels.rs). Each panel's UI is drawn
// into a texture of its own by uiVertex and uiFragment, at the size it shows
// on screen, and uiPanelMipMain makes its mips. Each view then draws its
// panels over the finished picture, after post-processing, so their colours
// are as designed, one rectangle each from buffers().uiPanels. A panel is
// hidden where the scene is in front of it, read from the scene's depth as
// debug draw reads it. push.textures: the scene's depth texture, and the
// view's offset in the output (x, y); push.size is the depth's size and
// push.output the view's (width | height << 16), which differ when the scene
// is drawn smaller and upscaled.

struct UiPanelOut
{
    float4 position : SV_Position;
    // Across and down the panel, from 0 to 1.
    float2 uv : UV;
    nointerpolation uint texture : TEXTURE;
};

[shader("vertex")]
UiPanelOut uiPanelVertex(uint vertexId : SV_VertexID)
{
    UiPanel panel = buffers()->uiPanels[vertexId / 6];
    float2 corner = UI_CORNERS[vertexId % 6];
    float3 world = panel.origin + panel.across * corner.x + panel.down * corner.y;
    UiPanelOut o;
    o.position = transform(push.frame->unjitteredViewProj, float4(world, 1.0));
    o.uv = corner;
    o.texture = panel.texture;
    return o;
}

[shader("fragment")]
float4 uiPanelFragment(UiPanelOut i) : SV_Target
{
    // Read before anything is thrown away, so the mip is picked from neighbouring pixels that all ran.
    float4 color = textures[NonUniformResourceIndex(i.texture)].Sample(samplers[SAMPLER_LINEAR_CLAMP], i.uv);
    float2 view = float2(push.output & 0xFFFF, push.output >> 16);
    float2 inView = i.position.xy - float2(push.textures.yz);
    int2 pixel = int2(inView * float2(push.size) / max(view, 1.0));
    float scene = textures[NonUniformResourceIndex(push.textures.x)].Load(int3(pixel, 0)).r;
    // Reverse-Z: nearer is larger. A little slack keeps a panel on a wall from flickering into it.
    if (i.position.z < scene * 0.999)
    {
        discard;
    }
    // Drawn after anti-aliasing, a slanted edge would step from pixel to pixel: the pixels along it fade over
    // the half pixel inside it instead.
    float2 inside = min(i.uv, 1.0 - i.uv) / max(fwidth(i.uv), 1e-6);
    return color * saturate(min(inside.x, inside.y) + 0.5);
}

// One mip of a panel's texture from the one before, twice its size: the
// average of the four texels under it, its colour premultiplied, so a
// see-through edge does not darken. push.texture is the texture, push.level
// the mip read, push.output this mip's storage image, push.size its size.
[shader("compute")]
[numthreads(8, 8, 1)]
void uiPanelMipMain(uint3 id : SV_DispatchThreadID)
{
    if (any(id.xy >= push.size))
    {
        return;
    }
    float2 uv = (float2(id.xy) + 0.5) / float2(push.size);
    Texture2D texture = textures[NonUniformResourceIndex(push.texture)];
    storageImagesRgba16f[push.output][id.xy] =
        texture.SampleLevel(samplers[SAMPLER_LINEAR_CLAMP], uv, float(push.level));
}
