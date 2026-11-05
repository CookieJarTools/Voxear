// Glass, drawn after the opaque scene and the sky, pane by pane from the
// farthest, over a copy of the picture behind it with blurred mips
// (glassCopyMain). Each pane shows what lies behind it, bent by how it tilts
// to the view, tinted by its colour and blurred as it roughens, with its
// surroundings and the lights reflected in it, more at grazing angles, as
// glass reflects them. Panes neither write depth nor cast shadows, so what
// follows (fog, depth of field) sees what lies behind them.

#include "shading.slang"

// How far a pane bends what lies behind it, in screen widths, for each unit
// its normal tilts across the view and each unit of index above 1.
static const float GLASS_BEND = 0.06;
// How many of the copy's mips the roughest glass blurs through.
static const float GLASS_BLUR_MIPS = 6.0;

// One mip of the copy at `uv`, read as a cubic B-spline rather than
// bilinearly: four bilinear reads weighted so the texels blend smoothly into
// one another (Sigg and Hadwiger, GPU Gems 2, 20). Read bilinearly, a
// coarse mip's texels showed through as squares, and a lamp behind
// frosted glass was a block of pixels rather than a soft glow.
float3 glassSmooth(Texture2D texture, float2 uv, float level)
{
    uint2 full;
    uint mips;
    texture.GetDimensions(0, full.x, full.y, mips);
    float2 size = max(float2(full >> uint(level)), 1.0);
    float2 texel = uv * size - 0.5;
    float2 f = frac(texel);
    float2 corner = floor(texel) + 0.5;
    float2 f2 = f * f;
    float2 f3 = f2 * f;
    float2 w0 = (1.0 - 3.0 * f + 3.0 * f2 - f3) / 6.0;
    float2 w1 = (4.0 - 6.0 * f2 + 3.0 * f3) / 6.0;
    float2 w2 = (1.0 + 3.0 * f + 3.0 * f2 - 3.0 * f3) / 6.0;
    float2 w3 = f3 / 6.0;
    float2 g0 = w0 + w1;
    float2 g1 = w2 + w3;
    float2 t0 = (corner - 1.0 + w1 / g0) / size;
    float2 t1 = (corner + 1.0 + w3 / g1) / size;
    SamplerState linear = samplers[SAMPLER_LINEAR_CLAMP];
    return (texture.SampleLevel(linear, float2(t0.x, t0.y), level).rgb * g0.x
            + texture.SampleLevel(linear, float2(t1.x, t0.y), level).rgb * g1.x) * g0.y
        + (texture.SampleLevel(linear, float2(t0.x, t1.y), level).rgb * g0.x
            + texture.SampleLevel(linear, float2(t1.x, t1.y), level).rgb * g1.x) * g1.y;
}

// As glassSmooth, between the two mips either side of `blur`.
float3 glassSmoothBetween(Texture2D texture, float2 uv, float blur)
{
    float below = floor(blur);
    float3 near = glassSmooth(texture, uv, below);
    float part = blur - below;
    if (part < 0.01)
    {
        return near;
    }
    return lerp(near, glassSmooth(texture, uv, below + 1.0), part);
}

// What lies behind a pane at `uv`, blurred through the copy's mips as far as
// `blur`: a ring of samples a texel of that mip apart round the middle, so a
// coarse mip reads as a soft blur rather than as blocks.
float3 glassBehind(uint copy, float2 uv, float blur)
{
    Texture2D texture = textures[NonUniformResourceIndex(copy)];
    if (blur < 0.5)
    {
        return texture.SampleLevel(samplers[SAMPLER_LINEAR_CLAMP], uv, blur).rgb;
    }
    float3 total = glassSmoothBetween(texture, uv, blur);
    uint2 size;
    uint mips;
    texture.GetDimensions(0, size.x, size.y, mips);
    float2 step = exp2(blur) / float2(size);
    static const uint RING = 8;
    for (uint i = 0; i < RING; i++)
    {
        float angle = (float(i) + 0.5) * (2.0 * PI / float(RING));
        total += glassSmoothBetween(texture, uv + float2(cos(angle), sin(angle)) * step, blur);
    }
    return total / float(RING + 1);
}

[shader("vertex")]
VertexOut glassVertex(uint vertexId : SV_VertexID)
{
    // Each pane is a draw of its own, with its instance in the push constant's list.
    return forwardVertex(push.list, 0, 0, 0, vertexId);
}

[shader("fragment")]
float4 glassFragment(VertexOut input) : SV_Target0
{
    Frame* frame = push.frame;
    Material material = buffers()->materials[input.material];
    float4 tint = material.baseColor * sampleTexture(material.baseColorTexture, input.uv);
    float4 metalRough = sampleTexture(material.metallicRoughnessTexture, input.uv);

    Surface s;
    s.normal = surfaceNormal(input, material);
    s.view = normalize(frame->cameraPosition - input.worldPosition);
    s.roughness = clamp(material.roughness * metalRough.g, 0.02, 1.0);
    s.diffuse = 0.0;
    float ior = max(material.ior, 1.0);
    float r0 = (ior - 1.0) / (ior + 1.0);
    s.f0 = r0 * r0;
    float nv = saturate(dot(s.normal, s.view));
    float3 fresnel = s.f0 + (max(float3(1.0 - s.roughness), s.f0) - s.f0) * pow(1.0 - nv, 5.0);

    // What lies behind, moved as the pane's normal tilts away from the view, and blurred as it roughens.
    float3 viewNormal = transform(frame->view, float4(s.normal, 0.0)).xyz;
    float2 bend = float2(viewNormal.x, -viewNormal.y) * GLASS_BEND * (ior - 1.0);
    float2 uv = input.position.xy / float2(frame->viewport) - bend;
    float blur = sqrt(s.roughness) * GLASS_BLUR_MIPS;
    float3 behind = glassBehind(push.texture, uv, blur);
    float3 through = behind * tint.rgb * (1.0 - fresnel);

    bool atmosphere = frame->lighting.sky == SKY_ATMOSPHERE;
    float3 sunLight = atmosphere ? sunThroughAtmosphere(frame) : frame->sunColor;
    float3 sunThrough = sunLight * sunCookieAt(frame, input.worldPosition);
    float shadow = sunShadow(frame, input.worldPosition, s.normal);
    float3 ambient = ambientLight(frame, input.worldPosition, s, sunLight, atmosphere);
    float3 direction = reflect(-s.view, s.normal);
    float3 surface = surroundingsReflection(frame, input.worldPosition, s.normal, direction, s.roughness, ambient, atmosphere) * fresnel;
    surface += shade(s, -frame->sunDirection, sunThrough) * shadow;
    surface += localLights(frame, s, input.worldPosition, input.position.xy);
    surface += material.emissive * sampleTexture(material.emissiveTexture, input.uv).rgb;
    if (atmosphere)
    {
        // What lies behind is hazed already; the pane's own light is hazed as far as the pane.
        float4 haze = aerialPerspectiveAt(frame, input.position.xy / float2(frame->viewport), length(input.worldPosition - frame->cameraPosition));
        surface *= haze.a;
    }
    float3 color = through + surface;
    if (frame->debugView == DEBUG_VIEW_NORMALS)
    {
        color = s.normal * 0.5 + 0.5;
    }
    else if (frame->debugView == DEBUG_VIEW_UNLIT)
    {
        color = behind * tint.rgb;
    }
    else if (frame->debugView == DEBUG_VIEW_GLOBAL_ILLUMINATION)
    {
        color = ambient;
    }
    return float4(color, 1.0);
}

struct GlassShadowOut
{
    float4 position : SV_Position;
    float3 world : WORLD;
    float2 uv : UV;
    nointerpolation uint material : MATERIAL;
};

struct GlassShadowTargets
{
    float4 light : SV_Target0;
    float smoothness : SV_Target1;
};

// A pane drawn into the glass shadow atlas, from the sun's cascade
// (`list` LIST_SHADOW and after) or a local light's view (`list` its own
// list, `level` the view); its instance in `source`.
[shader("vertex")]
GlassShadowOut glassShadowVertex(uint vertexId : SV_VertexID)
{
    Instance instance = buffers()->instances[push.source];
    Mesh mesh = buffers()->meshes[instance.mesh];
    Vertex v = buffers()->vertices[mesh.vertexOffset + vertexId];
    float4 world = transform(instance.model, float4(v.position, 1.0));
    GlassShadowOut output;
    output.position = transform(shadowViewProj(push.frame, push.list, push.level), world);
    output.world = world.xyz;
    output.uv = v.uv;
    output.material = instance.material;
    return output;
}

// What a pane lets through of the light, which blending multiplies by what
// the panes before it let through, and where it is, which blending keeps
// the nearest of: its depth for the sun, its distance from a local light in
// metres. And its smoothness, which says how far the light it lets through
// spreads (glassLight in lights.slang).
[shader("fragment")]
GlassShadowTargets glassShadowFragment(GlassShadowOut input)
{
    Frame* frame = push.frame;
    Material material = buffers()->materials[input.material];
    float3 tint = (material.baseColor * sampleTexture(material.baseColorTexture, input.uv)).rgb;
    float roughness = material.roughness * sampleTexture(material.metallicRoughnessTexture, input.uv).g;
    float ior = max(material.ior, 1.0);
    float r0 = (ior - 1.0) / (ior + 1.0);
    // Reflected off as the light goes in, and again as it comes out.
    float passes = (1.0 - r0 * r0) * (1.0 - r0 * r0);
    float depth = input.position.z;
    if (push.list >= LIST_LOCAL_SHADOW)
    {
        depth = length(input.world - frame->localShadows.casters[push.list - LIST_LOCAL_SHADOW].xyz);
    }
    GlassShadowTargets output;
    output.light = float4(saturate(tint) * passes, depth);
    output.smoothness = 1.0 - saturate(roughness);
    return output;
}

// One mip of the glass shadow atlas from the one before, mip `level`: the
// light let through averaged over each 2x2 texels, as it spreads over them,
// and the nearest pane among them and the least smoothness kept.
// textures: x the smoothness, y the storage image of this mip of it.
[shader("compute")]
[numthreads(8, 8, 1)]
void glassShadowMipMain(uint3 id : SV_DispatchThreadID)
{
    if (any(id.xy >= push.size))
    {
        return;
    }
    Texture2D light = textures[NonUniformResourceIndex(push.texture)];
    Texture2D smoothness = textures[NonUniformResourceIndex(push.textures.x)];
    float3 total = 0.0;
    float nearest = 65504.0;
    float smooth = 1.0;
    for (uint i = 0; i < 4; i++)
    {
        int3 at = int3(int2(id.xy * 2 + uint2(i % 2, i / 2)), int(push.level));
        float4 through = light.Load(at);
        total += through.rgb;
        nearest = min(nearest, through.a);
        smooth = min(smooth, smoothness.Load(at).r);
    }
    storageImagesRgba16f[push.output][id.xy] = float4(total * 0.25, nearest);
    storageImagesR32f[push.textures.y][id.xy] = smooth;
}

// One mip of the copy of the picture glass is drawn over: the first the
// picture itself, each after it the one before filtered down with the
// thirteen reads Jimenez gives for bloom (2014), which weigh a wide tent
// round each texel. Each the average of the four texels under it, a bright
// lamp stayed in one texel or jumped to the next as it moved, and showed
// through frosted glass as a square. The first of them weighs each group of
// four by how dim it is (Karis's average), as bloom does: a lamp's highlight
// on a shiny ball, a pixel or two wide, comes and goes as the camera moves,
// and blurred whole it flashed through frosted glass as a bright patch.
[shader("compute")]
[numthreads(8, 8, 1)]
void glassCopyMain(uint3 id : SV_DispatchThreadID)
{
    if (any(id.xy >= push.size))
    {
        return;
    }
    Texture2D texture = textures[NonUniformResourceIndex(push.texture)];
    SamplerState linear = samplers[SAMPLER_LINEAR_CLAMP];
    float2 uv = (float2(id.xy) + 0.5) / float2(push.size);
    // The first mip copies the picture, which is as big as it is; each after reads the mip `level`, twice its size.
    uint2 full;
    uint mips;
    texture.GetDimensions(0, full.x, full.y, mips);
    if (all(full == push.size))
    {
        storageImagesRgba16f[push.output][id.xy] = texture.SampleLevel(linear, uv, 0.0);
        return;
    }
    // A texel of the mip read, which is twice as fine as this one.
    float2 t = 0.5 / float2(push.size);
    float level = float(push.level);
    let at = (float2 offset) => texture.SampleLevel(linear, uv + offset * t, level);
    float4 a = at(float2(-2.0, -2.0)), b = at(float2(0.0, -2.0)), c = at(float2(2.0, -2.0));
    float4 d = at(float2(-1.0, -1.0)), e = at(float2(1.0, -1.0));
    float4 f = at(float2(-2.0, 0.0)), g = at(float2(0.0, 0.0)), h = at(float2(2.0, 0.0));
    float4 i = at(float2(-1.0, 1.0)), j = at(float2(1.0, 1.0));
    float4 k = at(float2(-2.0, 2.0)), l = at(float2(0.0, 2.0)), m = at(float2(2.0, 2.0));
    // The middle four, and the four groups round it, which together weigh the tent.
    float4 groups[5] = { d + e + i + j, a + b + f + g, b + c + g + h, f + g + k + l, g + h + l + m };
    float shares[5] = { 0.5, 0.125, 0.125, 0.125, 0.125 };
    float4 color = 0.0;
    float total = 0.0;
    for (uint n = 0; n < 5; n++)
    {
        float4 group = groups[n] * 0.25;
        float weight = shares[n] * (push.level == 0 ? 1.0 / (1.0 + dot(group.rgb, float3(0.2126, 0.7152, 0.0722))) : 1.0);
        color += group * weight;
        total += weight;
    }
    storageImagesRgba16f[push.output][id.xy] = color / total;
}
