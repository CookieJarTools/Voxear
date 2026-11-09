// Ambient occlusion as the opaque passes read it, and the depth passes that
// draw the frame's depth before them, so GTAO (ambient_occlusion.slang) has
// all of it to work from. Included by shading.slang.

#pragma once

#include "common.slang"
#include "custom.slang"

// How much of the sky's ambient light reaches a surface: the diffuse share,
// tinted toward post.aoTint where it is occluded, and the specular share for
// its reflections.
struct Occlusion
{
    float3 diffuse;
    float specular;
};

Occlusion ambientOcclusion(Frame* frame, float2 pixel, float nv, float roughness)
{
    Occlusion o;
    o.diffuse = 1.0;
    o.specular = 1.0;
    if (frame->post.aoTexture == NO_TEXTURE)
    {
        return o;
    }
    float visibility = textures[NonUniformResourceIndex(frame->post.aoTexture)].Load(int3(int2(pixel), 0)).r;
    visibility = pow(max(visibility, 1e-3), frame->post.aoStrength);
    // Written so a visibility of exactly 1 leaves the light exactly as it was.
    o.diffuse = 1.0 - (1.0 - frame->post.aoTint) * (1.0 - visibility);
    // Lagarde and de Rousiers' specular occlusion: smooth surfaces reflect
    // through a narrower cone, so occlusion hides less of their reflections.
    o.specular = saturate(pow(nv + visibility, exp2(-16.0 * roughness - 1.0)) - 1.0 + visibility);
    return o;
}

// The depth passes: the opaque lists as depth alone, discarding what the
// opaque passes discard (LOD fades and alpha cut-outs), so their depth is
// the same to the bit and those passes shade only what it kept; and each
// pixel's normal, folded flat, and roughness beside it, so reflections pass
// over what is too rough to show them and trace from the surface's own
// smooth normal, and in alpha 0 where two LODs crossfade, 1 elsewhere
// (SURFACE_FORMAT in src/pipelines.rs).
[shader("fragment")]
float4 depthFragment(VertexOut input) : SV_Target0
{
    ditherLodFade(input);
    Material material = buffers()->materials[input.material];
    bool custom = material.shader != STANDARD_SHADER;
    if (material.alphaCutoff > 0.0 && !custom)
    {
        float alpha = material.baseColor.a * sampleTexture(material.baseColorTexture, input.uv).a;
        if (alpha < material.alphaCutoff)
        {
            discard;
        }
    }
    float roughness = material.roughness * sampleTexture(material.metallicRoughnessTexture, input.uv).g;
    // Wet or snowy plants are as rough as the opaque passes shade them.
    float3 albedo = material.baseColor.rgb;
    float3 origin = buffers()->instances[input.instance].model.c3.xyz;
    seasonalSurface(push.frame, material, origin, shapeNormal(input, material), albedo, roughness);
    float3 normal = surfaceNormal(input, material);
    // A custom shader's surface, cut out and as rough as shading has it.
    if (custom)
    {
        float4 base = material.baseColor * sampleTexture(material.baseColorTexture, input.uv);
        float metallic = material.metallic * sampleTexture(material.metallicRoughnessTexture, input.uv).b;
        SurfaceOutput surface = materialSurface(albedo, base.a, normal, metallic, roughness, material.emissive);
        surface = customSurfaceAt(push.frame, input, surface, origin, SOLID_THICKNESS, false);
        if (material.alphaCutoff > 0.0 && surface.alpha < material.alphaCutoff)
        {
            discard;
        }
        normal = surface.normal;
        roughness = surface.roughness;
    }
    // As rough as shading draws it, so reflections fade from bumpy shiny detail just as its highlights do.
    roughness = smoothedRoughness(push.frame, normal, roughness);
    // Marked while its LODs crossfade, as the depth then is either one's, pixel by pixel (ambient_occlusion.slang).
    float settled = input.fade == 0 ? 1.0 : 0.0;
    return float4(octEncode(normal) * 0.5 + 0.5, saturate(roughness), settled);
}

// Whether the contact shadows pass found something between a pixel and the
// sun, close by: 0 in such a shadow, 1 out of it, and 1 without the pass.
float contactShadow(Frame* frame, float2 pixel)
{
    if (frame->post.contactTexture == NO_TEXTURE)
    {
        return 1.0;
    }
    return textures[NonUniformResourceIndex(frame->post.contactTexture)].Load(int3(int2(pixel), 0)).r;
}
