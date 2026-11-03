// Post-processing, as fullscreen passes over the HDR image:
// fog, TAA, depth of field, motion blur, the bloom chain, the composite
// (exposure, grading, tonemapping, vignette, lens effects, grain, dither) and
// the finish (FXAA or sharpening). Each pass reads textures named in
// push.textures; see src/post_passes.rs for the order.

#include "common.slang"
#include "volume.slang"

static const float3 LUMA = float3(0.2126, 0.7152, 0.0722);
static const uint BLOOM_LEVELS = 6;

struct PostOut
{
    float4 position : SV_Position;
    float2 uv : TEXCOORD0;
};

[shader("vertex")]
PostOut postVertex(uint vertexId : SV_VertexID)
{
    PostOut o;
    float2 ndc = fullscreenNdc(vertexId);
    o.position = float4(ndc, 0.0, 1.0);
    o.uv = ndc * 0.5 + 0.5;
    return o;
}

float4 sampleLinear(uint texture, float2 uv)
{
    return textures[NonUniformResourceIndex(texture)].SampleLevel(samplers[SAMPLER_LINEAR_CLAMP], uv, 0.0);
}

float4 load(uint texture, int2 pixel)
{
    int2 size = int2(push.frame->viewport);
    return textures[NonUniformResourceIndex(texture)].Load(int3(clamp(pixel, int2(0), size - 1), 0));
}

float luminance(float3 color)
{
    return dot(color, LUMA);
}

// ---------------------------------------------------------------- Fog

// The share of light exponential height fog lets through along a ray from
// the camera, where the fog's density at the camera is `density`.
float heightFogThrough(float density, float falloff, float3 ray)
{
    float rise = falloff * ray.y;
    float heightTerm = abs(rise) > 1e-4 ? (1.0 - exp(-rise)) / rise : 1.0;
    return exp(-density * length(ray) * heightTerm);
}

// Exponential height fog, integrated along each view ray, glowing towards the
// sun. The sky is left alone: it already fades into the horizon.
[shader("fragment")]
float4 fogFragment(PostOut input) : SV_Target
{
    Frame* frame = push.frame;
    float depth = load(push.textures.x, int2(input.position.xy)).r;
    if (depth <= 0.0)
    {
        discard;
    }
    float4 point = transform(frame->invViewProj, float4(input.uv * 2.0 - 1.0, depth, 1.0));
    float3 ray = point.xyz / point.w - frame->cameraPosition;
    float distance = length(ray);
    float3 direction = ray / distance;

    float falloff = max(frame->post.fogHeightFalloff, 1e-4);
    float density = frame->post.fogDensity * exp(-falloff * (frame->cameraPosition.y - frame->post.fogBaseHeight));
    float amount = 1.0 - heightFogThrough(density, falloff, ray);

    float glow = pow(saturate(dot(direction, -frame->sunDirection)), 8.0) * frame->post.fogSunGlow;
    float3 color = frame->post.fogColor + frame->sunColor * glow * 0.15;
    return float4(color, saturate(amount));
}

// Volumetric fog, as the volume passes (volumetric.slang) worked it out,
// laid over the scene: what the fog adds, and the scene through it as
// premultiplied alpha. Beyond the volume's reach, the height fog above
// carries on from where it ends; the sky gets the volume's whole depth.
// textures: x depth, y the integrated volume.
[shader("fragment")]
float4 volumetricFogFragment(PostOut input) : SV_Target
{
    Frame* frame = push.frame;
    float far = frame->post.fogVolumeFar;
    float depth = load(push.textures.x, int2(input.position.xy)).r;
    if (depth <= 0.0)
    {
        float4 sky = volumeAt(push.textures.y, input.uv, far, far);
        return float4(sky.rgb, 1.0 - sky.a);
    }
    float viewDepth = frame->near / depth;
    float4 fog = volumeAt(push.textures.y, input.uv, min(viewDepth, far), far);
    if (viewDepth <= far)
    {
        return float4(fog.rgb, 1.0 - fog.a);
    }

    // The height fog from the volume's far side on: its share over the whole
    // ray, less its share over the part the volume covers.
    float4 point = transform(frame->invViewProj, float4(input.uv * 2.0 - 1.0, depth, 1.0));
    float3 ray = point.xyz / point.w - frame->cameraPosition;
    float falloff = max(frame->post.fogHeightFalloff, 1e-4);
    float density = frame->post.fogDensity * exp(-falloff * (frame->cameraPosition.y - frame->post.fogBaseHeight));
    float throughAll = heightFogThrough(density, falloff, ray);
    float throughNear = heightFogThrough(density, falloff, ray * (far / viewDepth));
    float beyond = 1.0 - throughAll / max(throughNear, 1e-6);
    float glow = pow(saturate(dot(normalize(ray), -frame->sunDirection)), 8.0) * frame->post.fogSunGlow;
    float3 beyondColor = (frame->post.fogColor + frame->sunColor * glow * 0.15) * beyond;
    return float4(fog.rgb + fog.a * beyondColor, 1.0 - fog.a * (1.0 - beyond));
}

// ---------------------------------------------------------------- TAA

// Clipping and blending happen in a tonemapped space, so one bright pixel
// cannot dominate its neighbourhood (less flicker on highlights).
float3 compress(float3 c)
{
    return c / (1.0 + max(c.r, max(c.g, c.b)));
}

float3 expand(float3 c)
{
    return c / max(1.0 - max(c.r, max(c.g, c.b)), 1e-4);
}

float3 toYCoCg(float3 c)
{
    return float3(dot(c, float3(0.25, 0.5, 0.25)), dot(c, float3(0.5, 0.0, -0.5)), dot(c, float3(-0.25, 0.5, -0.25)));
}

float3 fromYCoCg(float3 c)
{
    return float3(c.x + c.y - c.z, c.x + c.z, c.x - c.y - c.z);
}

float3 clipToBox(float3 history, float3 low, float3 high)
{
    float3 centre = (low + high) * 0.5;
    float3 extent = (high - low) * 0.5 + 1e-5;
    float3 offset = history - centre;
    float3 units = abs(offset / extent);
    float largest = max(units.x, max(units.y, units.z));
    return largest > 1.0 ? centre + offset / largest : history;
}

// Catmull-Rom history lookup from nine bilinear taps: sharper than plain
// bilinear, so TAA does not soften the image over time.
float3 sampleHistory(uint texture, float2 uv, float2 size)
{
    float2 position = uv * size;
    float2 base = floor(position - 0.5) + 0.5;
    float2 f = position - base;
    float2 w0 = f * (-0.5 + f * (1.0 - 0.5 * f));
    float2 w1 = 1.0 + f * f * (-2.5 + 1.5 * f);
    float2 w2 = f * (0.5 + f * (2.0 - 1.5 * f));
    float2 w3 = f * f * (-0.5 + 0.5 * f);
    float2 w12 = w1 + w2;
    float2 p0 = (base - 1.0) / size;
    float2 p3 = (base + 2.0) / size;
    float2 p12 = (base + w2 / w12) / size;
    float3 result = 0.0;
    result += sampleLinear(texture, float2(p0.x, p0.y)).rgb * w0.x * w0.y;
    result += sampleLinear(texture, float2(p12.x, p0.y)).rgb * w12.x * w0.y;
    result += sampleLinear(texture, float2(p3.x, p0.y)).rgb * w3.x * w0.y;
    result += sampleLinear(texture, float2(p0.x, p12.y)).rgb * w0.x * w12.y;
    result += sampleLinear(texture, float2(p12.x, p12.y)).rgb * w12.x * w12.y;
    result += sampleLinear(texture, float2(p3.x, p12.y)).rgb * w3.x * w12.y;
    result += sampleLinear(texture, float2(p0.x, p3.y)).rgb * w0.x * w3.y;
    result += sampleLinear(texture, float2(p12.x, p3.y)).rgb * w12.x * w3.y;
    result += sampleLinear(texture, float2(p3.x, p3.y)).rgb * w3.x * w3.y;
    return max(result, 0.0);
}

// textures: x current HDR, y last frame's result, z velocity, w depth.
[shader("fragment")]
float4 taaFragment(PostOut input) : SV_Target
{
    Frame* frame = push.frame;
    int2 pixel = int2(input.position.xy);
    float3 first = 0.0;
    float3 second = 0.0;
    float3 centre = 0.0;
    float nearest = 0.0;
    int2 nearestOffset = int2(0);
    for (int y = -1; y <= 1; y++)
    {
        for (int x = -1; x <= 1; x++)
        {
            float3 c = toYCoCg(compress(load(push.textures.x, pixel + int2(x, y)).rgb));
            first += c;
            second += c * c;
            centre = (x == 0 && y == 0) ? c : centre;
            // Reverse-Z: the largest depth is the nearest surface, whose motion wins at edges.
            float depth = load(push.textures.w, pixel + int2(x, y)).r;
            nearestOffset = depth > nearest ? int2(x, y) : nearestOffset;
            nearest = max(nearest, depth);
        }
    }
    if (frame->taaReset != 0)
    {
        return float4(expand(fromYCoCg(centre)), 1.0);
    }

    float2 velocity = load(push.textures.z, pixel + nearestOffset).xy;
    float2 historyUv = input.uv - velocity;
    if (any(historyUv < 0.0) || any(historyUv > 1.0))
    {
        return float4(expand(fromYCoCg(centre)), 1.0);
    }

    float3 mean = first / 9.0;
    float3 deviation = sqrt(max(second / 9.0 - mean * mean, 0.0));
    float3 history = toYCoCg(compress(sampleHistory(push.textures.y, historyUv, float2(frame->viewport))));
    history = clipToBox(history, mean - 1.25 * deviation, mean + 1.25 * deviation);

    // Keep less history when things move fast, so motion stays crisp.
    float speed = length(velocity * float2(frame->viewport));
    float keep = frame->post.taaHistory * lerp(1.0, 0.8, saturate(speed / 32.0));
    return float4(expand(fromYCoCg(lerp(centre, history, keep))), 1.0);
}

// ---------------------------------------------------------------- Depth of field

float linearDepth(float depth)
{
    // Infinite reverse-Z: depth = near / distance.
    return push.frame->near / max(depth, 1e-7);
}

// Blur radius in pixels, scaled from 1080p to the current resolution.
float circleOfConfusion(float depth)
{
    Frame* frame = push.frame;
    float distance = linearDepth(depth);
    float blur = abs(1.0 - frame->post.dofFocus / distance) * 24.0 / max(frame->post.dofAperture, 0.5);
    return min(blur, frame->post.dofMaxBlur) * float(frame->viewport.y) / 1080.0;
}

// A gather over a golden-angle spiral. Nearer samples blur over this pixel;
// farther ones only count as far as this pixel's own blur reaches, so sharp
// foreground edges do not smear into a blurry background.
[shader("fragment")]
float4 dofFragment(PostOut input) : SV_Target
{
    Frame* frame = push.frame;
    int2 pixel = int2(input.position.xy);
    float centreDepth = load(push.textures.y, pixel).r;
    float centreBlur = circleOfConfusion(centreDepth);
    float radius = frame->post.dofMaxBlur * float(frame->viewport.y) / 1080.0;
    float3 sum = load(push.textures.x, pixel).rgb;
    float weight = 1.0;
    static const uint TAPS = 48;
    for (uint i = 1; i < TAPS; i++)
    {
        float r = sqrt(float(i) / float(TAPS)) * radius;
        float angle = float(i) * 2.39996323;
        int2 offset = int2(round(float2(cos(angle), sin(angle)) * r));
        float depth = load(push.textures.y, pixel + offset).r;
        float reach = depth >= centreDepth ? circleOfConfusion(depth) : min(circleOfConfusion(depth), centreBlur);
        float w = saturate(reach - r + 1.0);
        sum += load(push.textures.x, pixel + offset).rgb * w;
        weight += w;
    }
    return float4(sum / weight, 1.0);
}

// ---------------------------------------------------------------- Motion blur

// Samples along each pixel's motion, like a camera shutter.
[shader("fragment")]
float4 motionBlurFragment(PostOut input) : SV_Target
{
    Frame* frame = push.frame;
    float2 velocity = load(push.textures.y, int2(input.position.xy)).xy * frame->post.motionBlur;
    float pixels = length(velocity * float2(frame->viewport));
    if (pixels < 0.5)
    {
        return load(push.textures.x, int2(input.position.xy));
    }
    velocity *= min(1.0, 40.0 / pixels);
    static const uint TAPS = 12;
    float3 sum = 0.0;
    for (uint i = 0; i < TAPS; i++)
    {
        sum += sampleLinear(push.textures.x, input.uv + velocity * ((float(i) + 0.5) / float(TAPS) - 0.5)).rgb;
    }
    return float4(sum / float(TAPS), 1.0);
}

// ---------------------------------------------------------------- Bloom

// The 13-tap downsample from Call of Duty: Advanced Warfare (Jimenez 2014).
// The first level weights each 2x2 group by brightness (Karis average) so a
// single very bright pixel cannot become a flickering blob.
// The frame debugger's view of an image: push.texture is a copy of one of its mips, push.textures.x
// which channels to show (0 for RGB, 1 to 4 for red, green, blue or alpha, as grey) and
// push.textures.y how much to scale them by, as float bits. Values show as they are held, so
// negative ones, as in motion vectors, show by their size.
[shader("fragment")]
float4 frameDebugFragment(PostOut input) : SV_Target
{
    float4 value = textures[NonUniformResourceIndex(push.texture)].SampleLevel(samplers[SAMPLER_NEAREST_CLAMP], input.uv, 0.0);
    value = abs(value) * asfloat(push.textures.y);
    float3 shown = push.textures.x == 0 ? value.rgb : float3(value[push.textures.x - 1]);
    return float4(saturate(shown), 1.0);
}

[shader("fragment")]
float4 bloomDownFragment(PostOut input) : SV_Target
{
    float2 texel = 1.0 / float2(push.size);
    float2 uv = input.uv;
    uint source = push.textures.x;
    float3 a = sampleLinear(source, uv + texel * float2(-2.0, -2.0)).rgb;
    float3 b = sampleLinear(source, uv + texel * float2(0.0, -2.0)).rgb;
    float3 c = sampleLinear(source, uv + texel * float2(2.0, -2.0)).rgb;
    float3 d = sampleLinear(source, uv + texel * float2(-1.0, -1.0)).rgb;
    float3 e = sampleLinear(source, uv + texel * float2(1.0, -1.0)).rgb;
    float3 f = sampleLinear(source, uv + texel * float2(-2.0, 0.0)).rgb;
    float3 g = sampleLinear(source, uv).rgb;
    float3 h = sampleLinear(source, uv + texel * float2(2.0, 0.0)).rgb;
    float3 i = sampleLinear(source, uv + texel * float2(-1.0, 1.0)).rgb;
    float3 j = sampleLinear(source, uv + texel * float2(1.0, 1.0)).rgb;
    float3 k = sampleLinear(source, uv + texel * float2(-2.0, 2.0)).rgb;
    float3 l = sampleLinear(source, uv + texel * float2(0.0, 2.0)).rgb;
    float3 m = sampleLinear(source, uv + texel * float2(2.0, 2.0)).rgb;

    float3 groups[5] = { (d + e + i + j) * 0.25, (a + b + f + g) * 0.25, (b + c + g + h) * 0.25, (f + g + k + l) * 0.25, (g + h + l + m) * 0.25 };
    float weights[5] = { 0.5, 0.125, 0.125, 0.125, 0.125 };
    float3 result = 0.0;
    float total = 0.0;
    for (uint n = 0; n < 5; n++)
    {
        float w = weights[n] * (push.level == 0 ? 1.0 / (1.0 + luminance(groups[n])) : 1.0);
        result += groups[n] * w;
        total += w;
    }
    return float4(result / total, 1.0);
}

// A 3x3 tent upsample of the smaller level, added onto the larger one.
[shader("fragment")]
float4 bloomUpFragment(PostOut input) : SV_Target
{
    float2 texel = 1.0 / float2(push.size);
    uint source = push.textures.x;
    float2 uv = input.uv;
    float3 sum = sampleLinear(source, uv).rgb * 4.0;
    sum += (sampleLinear(source, uv + float2(texel.x, 0.0)).rgb + sampleLinear(source, uv - float2(texel.x, 0.0)).rgb) * 2.0;
    sum += (sampleLinear(source, uv + float2(0.0, texel.y)).rgb + sampleLinear(source, uv - float2(0.0, texel.y)).rgb) * 2.0;
    sum += sampleLinear(source, uv + texel).rgb + sampleLinear(source, uv - texel).rgb;
    sum += sampleLinear(source, uv + float2(texel.x, -texel.y)).rgb + sampleLinear(source, uv + float2(-texel.x, texel.y)).rgb;
    return float4(sum / 16.0 * push.frame->post.bloomScatter, 1.0);
}

// ---------------------------------------------------------------- Tonemapping

// AgX by Troy Sobotka, in the widely used polynomial form. Matrices are
// written as their columns.
float3 agxCurve(float3 x)
{
    float3 x2 = x * x;
    float3 x4 = x2 * x2;
    return 15.5 * x4 * x2 - 40.14 * x4 * x + 31.96 * x4 - 6.868 * x2 * x + 0.4298 * x2 + 0.1191 * x - 0.00232;
}

float3 agx(float3 c, float contrast, float saturation)
{
    float3 v = float3(0.842479062253094, 0.0423282422610123, 0.0423756549057051) * c.r
        + float3(0.0784335999999992, 0.878468636469772, 0.0784336) * c.g
        + float3(0.0792237451477643, 0.0791661274605434, 0.879142973793104) * c.b;
    static const float MIN_EV = -12.47393;
    static const float MAX_EV = 4.026069;
    v = (clamp(log2(max(v, 1e-10)), MIN_EV, MAX_EV) - MIN_EV) / (MAX_EV - MIN_EV);
    v = agxCurve(v);
    // The "look": contrast and saturation in AgX's own space.
    v = pow(max(v, 0.0), contrast);
    float l = luminance(v);
    v = l + saturation * (v - l);
    float3 o = float3(1.19687900512017, -0.0528968517574562, -0.0529716355144438) * v.r
        + float3(-0.0980208811401368, 1.15190312990417, -0.0980434501171241) * v.g
        + float3(-0.0990297440797205, -0.0989611768448433, 1.15107367264116) * v.b;
    return pow(max(o, 0.0), 2.2);
}

// Narkowicz's fit of the ACES filmic curve.
float3 aces(float3 x)
{
    return saturate((x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14));
}

// Khronos PBR Neutral: true material colours up to the highlights.
float3 pbrNeutral(float3 color)
{
    static const float START = 0.76;
    static const float DESATURATION = 0.15;
    float x = min(color.r, min(color.g, color.b));
    color -= x < 0.08 ? x - 6.25 * x * x : 0.04;
    float peak = max(color.r, max(color.g, color.b));
    if (peak < START)
    {
        return color;
    }
    float d = 1.0 - START;
    float newPeak = 1.0 - d * d / (peak + d - START);
    color *= newPeak / peak;
    float g = 1.0 - 1.0 / (DESATURATION * (peak - newPeak) + 1.0);
    return lerp(color, float3(newPeak), g);
}

float3 tonemap(float3 color, Post post)
{
    switch (post.tonemapper)
    {
    case 0: return agx(color, post.tonemapContrast, post.tonemapSaturation);
    case 1: return aces(color);
    case 2: return saturate(pbrNeutral(color));
    case 3: return saturate(color / (1.0 + luminance(color)));
    default: return saturate(color);
    }
}

// ---------------------------------------------------------------- Composite

// Small, even shifts along blue-orange (temperature) and green-magenta
// (tint), keeping brightness unchanged.
float3 whiteBalance(float3 color, float temperature, float tint)
{
    float3 balanced = color * float3(1.0 + 0.1 * temperature + 0.05 * tint, 1.0 - 0.1 * tint, 1.0 - 0.1 * temperature + 0.05 * tint);
    return balanced * (luminance(color) / max(luminance(balanced), 1e-5));
}

float hash(float2 p)
{
    return frac(sin(dot(p, float2(12.9898, 78.233))) * 43758.5453);
}

// Interleaved gradient noise: well distributed, so dithering looks like fine grain.
float gradientNoise(float2 pixel, uint frameIndex)
{
    pixel += float(frameIndex % 64) * 5.588238;
    return frac(52.9829189 * frac(dot(pixel, float2(0.06711056, 0.00583715))));
}

float3 toSrgb(float3 c)
{
    return select(c <= 0.0031308, c * 12.92, 1.055 * pow(max(c, 0.0), 1.0 / 2.4) - 0.055);
}

float3 fromSrgb(float3 c)
{
    return select(c <= 0.04045, c / 12.92, pow((max(c, 0.0) + 0.055) / 1.055, 2.4));
}

// Breaks up banding in 8-bit output: noise of one code value, added in sRGB
// where the steps are even.
float3 dither(float3 color, float2 pixel, uint frameIndex)
{
    float noise = gradientNoise(pixel, frameIndex) - 0.5;
    return fromSrgb(toSrgb(color) + noise / 255.0);
}

// The composite and the finish may draw into part of the output (the
// editor's Scene panel), so push.size holds where that part starts, to turn
// output pixels back into pixels of the view's own images.
int2 viewPixel(PostOut input)
{
    return int2(input.position.xy) - int2(push.size);
}

// textures: x HDR colour, y bloom (or NO_TEXTURE). push.output is 1 when
// this is the last pass, which then dithers.
[shader("fragment")]
float4 compositeFragment(PostOut input) : SV_Target
{
    Frame* frame = push.frame;
    Post post = frame->post;
    float2 uv = input.uv;

    float3 color;
    if (post.chromaticAberration > 0.0)
    {
        float2 shift = (uv - 0.5) * post.chromaticAberration * 0.02;
        color = float3(sampleLinear(push.textures.x, uv + shift).r, sampleLinear(push.textures.x, uv).g, sampleLinear(push.textures.x, uv - shift).b);
    }
    else
    {
        color = load(push.textures.x, viewPixel(input)).rgb;
    }

    if (post.bloomIntensity > 0.0 && push.textures.y != NO_TEXTURE)
    {
        // The upsample chain sums the levels weighted 1, scatter, scatter^2, ...
        float s = clamp(post.bloomScatter, 0.0, 0.999);
        float normalise = (1.0 - s) / (1.0 - pow(s, float(BLOOM_LEVELS)));
        color = lerp(color, sampleLinear(push.textures.y, uv).rgb * normalise, post.bloomIntensity);
    }

    float ev = post.autoExposure != 0 ? buffers()->exposure[0] : post.exposureCompensation;
    color *= frame->exposure * exp2(ev);

    color = whiteBalance(color, post.temperature, post.tint);
    color = 0.18 * pow(max(color, 0.0) / 0.18, post.gradingContrast);
    float l = luminance(color);
    color = max(l + post.gradingSaturation * (color - l), 0.0);

    color = tonemap(color, post);
    color = post.gain * (color + post.lift * (1.0 - color));
    color = pow(max(color, 0.0), 1.0 / max(post.gamma, 0.01));

    if (post.vignette > 0.0)
    {
        float aspect = float(frame->viewport.x) / float(frame->viewport.y);
        float2 d = (uv - 0.5) * float2(lerp(1.0, aspect, post.vignetteRoundness), 1.0);
        float corner = length(float2(lerp(0.5, 0.5 * aspect, post.vignetteRoundness), 0.5));
        float shade = pow(saturate(length(d) / corner), 1.0 + 4.0 * (1.0 - post.vignetteSmoothness));
        color *= 1.0 - post.vignette * shade;
    }

    if (post.grain > 0.0)
    {
        float noise = hash(floor(input.position.xy / max(post.grainSize, 1.0)) + float(frame->frameIndex % 1024) * 1.618) - 0.5;
        color += noise * post.grain * (0.25 + sqrt(luminance(color)));
    }

    color = saturate(color);
    if (push.output != 0)
    {
        color = dither(color, input.position.xy, frame->frameIndex);
    }
    return float4(color, 1.0);
}

// ---------------------------------------------------------------- Finish

float perceivedLuma(float3 color)
{
    return sqrt(luminance(color));
}

// FXAA in its classic compact form: finds the edge direction from the
// corners' brightness and blurs along it.
float3 fxaa(uint source, float2 uv, float2 texel)
{
    float3 m = sampleLinear(source, uv).rgb;
    float lumaM = perceivedLuma(m);
    float lumaNW = perceivedLuma(sampleLinear(source, uv + texel * float2(-1.0, -1.0)).rgb);
    float lumaNE = perceivedLuma(sampleLinear(source, uv + texel * float2(1.0, -1.0)).rgb);
    float lumaSW = perceivedLuma(sampleLinear(source, uv + texel * float2(-1.0, 1.0)).rgb);
    float lumaSE = perceivedLuma(sampleLinear(source, uv + texel * float2(1.0, 1.0)).rgb);
    float lumaMin = min(lumaM, min(min(lumaNW, lumaNE), min(lumaSW, lumaSE)));
    float lumaMax = max(lumaM, max(max(lumaNW, lumaNE), max(lumaSW, lumaSE)));
    if (lumaMax - lumaMin < max(0.0312, lumaMax * 0.125))
    {
        return m;
    }
    float2 direction = float2(-((lumaNW + lumaNE) - (lumaSW + lumaSE)), (lumaNW + lumaSW) - (lumaNE + lumaSE));
    float reduce = max((lumaNW + lumaNE + lumaSW + lumaSE) * 0.03125, 1.0 / 128.0);
    float scale = 1.0 / (min(abs(direction.x), abs(direction.y)) + reduce);
    direction = clamp(direction * scale, -8.0, 8.0) * texel;
    float3 a = 0.5 * (sampleLinear(source, uv + direction * (1.0 / 3.0 - 0.5)).rgb + sampleLinear(source, uv + direction * (2.0 / 3.0 - 0.5)).rgb);
    float3 b = a * 0.5 + 0.25 * (sampleLinear(source, uv - direction * 0.5).rgb + sampleLinear(source, uv + direction * 0.5).rgb);
    float lumaB = perceivedLuma(b);
    return (lumaB < lumaMin || lumaB > lumaMax) ? a : b;
}

// Contrast-adaptive sharpening (AMD FidelityFX CAS): sharpens flat areas
// more than edges, so it restores texture detail without halos.
float3 sharpen(uint source, int2 pixel, float sharpness)
{
    float3 a = load(source, pixel + int2(-1, -1)).rgb;
    float3 b = load(source, pixel + int2(0, -1)).rgb;
    float3 c = load(source, pixel + int2(1, -1)).rgb;
    float3 d = load(source, pixel + int2(-1, 0)).rgb;
    float3 e = load(source, pixel).rgb;
    float3 f = load(source, pixel + int2(1, 0)).rgb;
    float3 g = load(source, pixel + int2(-1, 1)).rgb;
    float3 h = load(source, pixel + int2(0, 1)).rgb;
    float3 i = load(source, pixel + int2(1, 1)).rgb;
    float3 low = min(min(min(d, e), min(f, b)), h);
    low += min(min(low, a), min(min(c, g), i));
    float3 high = max(max(max(d, e), max(f, b)), h);
    high += max(max(high, a), max(max(c, g), i));
    float3 amount = sqrt(saturate(min(low, 2.0 - high) / max(high, 1e-5)));
    float3 weight = amount * (-1.0 / lerp(8.0, 5.0, saturate(sharpness)));
    return saturate((b * weight + d * weight + f * weight + h * weight + e) / (1.0 + 4.0 * weight));
}

[shader("fragment")]
float4 finishFragment(PostOut input) : SV_Target
{
    Frame* frame = push.frame;
    float3 color = frame->post.fxaa != 0
        ? fxaa(push.textures.x, input.uv, 1.0 / float2(frame->viewport))
        : sharpen(push.textures.x, viewPixel(input), frame->post.sharpness);
    return float4(dither(color, input.position.xy, frame->frameIndex), 1.0);
}
