// Baked light (src/lightmaps.rs): what the level's bake keeps of the light
// falling on each surface, read in place of, or beside, the probes round the
// camera. An instance with a lightmap reads it at its lightmap UVs, with a
// directional bake bent by the surface's own normal, normal map and all; one
// without is lit by the light probes round it, blended for where it is, as
// nine spherical harmonics a colour.

#pragma once

// An instance's baked light, if it has any: false for none.
bool bakedInstance(uint instanceIndex, out BakedInstance baked)
{
    uint index = buffers()->instances[instanceIndex].baked;
    if (index == 0)
    {
        baked = (BakedInstance)0;
        return false;
    }
    baked = buffers()->bakedInstances[index - 1];
    return true;
}

// Whether an instance's own lightmap lights it.
bool lightmapped(BakedInstance baked)
{
    return (baked.flags & BAKED_LIGHTMAP) != 0;
}

// The nine real spherical harmonics, L2, at a direction, as src/lightmaps.rs's harmonics.
void bakeHarmonics(float3 d, out float y[9])
{
    y[0] = 0.282095;
    y[1] = 0.488603 * d.y;
    y[2] = 0.488603 * d.z;
    y[3] = 0.488603 * d.x;
    y[4] = 1.092548 * d.x * d.y;
    y[5] = 1.092548 * d.y * d.z;
    y[6] = 0.315392 * (3.0 * d.z * d.z - 1.0);
    y[7] = 1.092548 * d.x * d.z;
    y[8] = 0.546274 * (d.x * d.x - d.y * d.y);
}

// The light the probes round an instance give a surface facing `normal`.
float3 probeIrradiance(BakedInstance baked, float3 normal)
{
    float flat[28];
    for (uint row = 0; row < 7; row++)
    {
        flat[row * 4] = baked.sh[row].x;
        flat[row * 4 + 1] = baked.sh[row].y;
        flat[row * 4 + 2] = baked.sh[row].z;
        flat[row * 4 + 3] = baked.sh[row].w;
    }
    float y[9];
    bakeHarmonics(normal, y);
    float3 light = 0.0;
    for (uint i = 0; i < 9; i++)
    {
        light += float3(flat[i * 3], flat[i * 3 + 1], flat[i * 3 + 2]) * y[i];
    }
    return max(light, 0.0);
}

// The light a lightmap gives at its UVs, `uv` already in the lightmap: bent
// for a directional bake by how the surface's own `normal` faces the way its
// light mostly came, against how the surface the bake saw faced it.
float3 lightmapIrradiance(BakedInstance baked, float2 uv, float3 normal)
{
    float3 light = textures[NonUniformResourceIndex(baked.lightmap)].SampleLevel(samplers[SAMPLER_LINEAR_CLAMP], uv, 0.0).rgb;
    if (baked.directions != NO_TEXTURE)
    {
        float4 way = textures[NonUniformResourceIndex(baked.directions)].SampleLevel(samplers[SAMPLER_LINEAR_CLAMP], uv, 0.0);
        float3 along = way.xyz * 2.0 - 1.0;
        light *= (dot(normal, along) * 0.5 + 0.5) / max(way.w, 1e-3);
    }
    return light;
}

// An instance's baked light at a point of it facing `normal`: its
// lightmap's, at `lightmapUv` (its own lightmap UVs), or the probes'. False
// where it has neither.
bool bakedIrradiance(BakedInstance baked, float2 lightmapUv, float3 normal, out float3 light)
{
    light = 0.0;
    if (lightmapped(baked))
    {
        light = lightmapIrradiance(baked, lightmapUv * baked.scaleOffset.xy + baked.scaleOffset.zw, normal);
        return true;
    }
    if ((baked.flags & BAKED_PROBES) != 0)
    {
        light = probeIrradiance(baked, normal);
        return true;
    }
    return false;
}

// The light probes' light at a point, from the grid it was resampled on as
// the level's bake loaded (src/lightmaps.rs's ProbeVolume), blended from the
// eight cells round it, for a surface facing `normal`; or with no normal,
// the light from every way averaged. False outside the grid, past a cell
// from its edge, or with none.
bool probeVolumeLight(Frame* frame, float3 position, float3 normal, out float3 light)
{
    light = 0.0;
    uint3 size = frame->bake.volumeSize;
    if (size.x < 2 || frame->bake.source == GI_REALTIME)
    {
        return false;
    }
    float3 at = (position - frame->bake.volumeOrigin) / frame->bake.volumeCell;
    if (any(at < -1.0) || any(at > float3(size)))
    {
        return false;
    }
    at = clamp(at, 0.0, float3(size - 1));
    int3 low = min(int3(floor(at)), int3(size) - 2);
    float3 f = at - float3(low);
    float4 red = 0.0;
    float4 green = 0.0;
    float4 blue = 0.0;
    for (uint corner = 0; corner < 8; corner++)
    {
        int3 step = int3(corner & 1, (corner >> 1) & 1, (corner >> 2) & 1);
        int3 cell = low + step;
        float3 w = select(step == 1, f, 1.0 - f);
        float weight = w.x * w.y * w.z;
        uint index = ((uint(cell.z) * size.y + uint(cell.y)) * size.x + uint(cell.x)) * 3;
        red += frame->bake.volume[index] * weight;
        green += frame->bake.volume[index + 1] * weight;
        blue += frame->bake.volume[index + 2] * weight;
    }
    float4 basis = float4(0.282095, 0.488603 * normal.y, 0.488603 * normal.z, 0.488603 * normal.x);
    light = max(float3(dot(red, basis), dot(green, basis), dot(blue, basis)), 0.0) * frame->bake.strength;
    return true;
}

// The lightmap texel density view: a checker of the texels a bake gives a
// surface, green at the level's texels a metre, bluer for fewer and redder
// for more; grey where the light probes light it.
float3 lightmapDensityColor(Frame* frame, BakedInstance baked, float2 lightmapUv, float3 worldPosition, bool found)
{
    if (!found || (baked.flags & BAKED_RECEIVES) == 0 || baked.density <= 0.0)
    {
        return float3(0.35);
    }
    float2 texel = floor(lightmapUv * baked.density);
    float checker = ((uint(texel.x) + uint(texel.y)) & 1u) != 0 ? 1.0 : 0.7;
    // How the texels compare with what the level asks for, from the texel's size on screen against a metre's.
    float3 dx = ddx(worldPosition);
    float3 dy = ddy(worldPosition);
    float2 du = ddx(lightmapUv * baked.density);
    float2 dv = ddy(lightmapUv * baked.density);
    float metres = max(length(dx), length(dy));
    float texels = max(length(du), length(dv));
    float perMetre = texels / max(metres, 1e-6);
    float ratio = log2(max(perMetre, 1e-4) / max(frame->bake.texelsPerMetre, 1e-4));
    float3 tint = ratio < 0.0 ? lerp(float3(0.3, 0.85, 0.35), float3(0.25, 0.4, 1.0), saturate(-ratio))
        : lerp(float3(0.3, 0.85, 0.35), float3(1.0, 0.3, 0.2), saturate(ratio));
    return tint * checker;
}
