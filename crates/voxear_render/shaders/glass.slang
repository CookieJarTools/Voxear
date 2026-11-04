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

// What lies behind a pane at `uv`, blurred through the copy's mips as far as
// `blur`: a ring of samples a texel of that mip apart round the middle, so a
// coarse mip reads as a soft blur rather than as blocks.
float3 glassBehind(uint copy, float2 uv, float blur)
{
    Texture2D texture = textures[NonUniformResourceIndex(copy)];
    SamplerState linear = samplers[SAMPLER_LINEAR_CLAMP];
    float3 total = texture.SampleLevel(linear, uv, blur).rgb;
    if (blur < 0.5)
    {
        return total;
    }
    uint2 size;
    uint mips;
    texture.GetDimensions(0, size.x, size.y, mips);
    float2 step = exp2(blur) / float2(size);
    static const uint RING = 8;
    for (uint i = 0; i < RING; i++)
    {
        float angle = (float(i) + 0.5) * (2.0 * PI / float(RING));
        total += texture.SampleLevel(linear, uv + float2(cos(angle), sin(angle)) * step, blur).rgb;
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

// One mip of the copy of the picture glass is drawn over: the first the
// picture itself, each after it the average of the four texels under it in
// the one before.
[shader("compute")]
[numthreads(8, 8, 1)]
void glassCopyMain(uint3 id : SV_DispatchThreadID)
{
    if (any(id.xy >= push.size))
    {
        return;
    }
    float2 uv = (float2(id.xy) + 0.5) / float2(push.size);
    float4 color = textures[NonUniformResourceIndex(push.texture)].SampleLevel(samplers[SAMPLER_LINEAR_CLAMP], uv, float(push.level));
    storageImagesRgba16f[push.output][id.xy] = color;
}
