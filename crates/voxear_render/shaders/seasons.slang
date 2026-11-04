// Seasons for plants (src/seasons.rs): leaves take each season's colour,
// and plants take snow from above and darken when wet.
// Each plant turns a little earlier or later than its neighbours, and
// patches dry out, by noise over the level at its origin, so a whole plant
// changes together.

#pragma once

#include "common.slang"
#include "wind.slang"

bool isLeaves(Material material)
{
    return (material.flags & MATERIAL_FOLIAGE) != 0;
}

// Leaves, or anything else that moves in the wind, such as bark.
bool isPlant(Material material)
{
    return isLeaves(material) || material.windBend > 0.0 || material.windFlutter > 0.0;
}

// What a plant shows of the year: its leaves' colour, weighted by how far
// they turn to it, and that amount; and its snow.
struct PlantSeason
{
    float4 color;
    float snow;
};

// The year at a plant standing at `origin`, early or late by where it stands.
PlantSeason plantSeason(Frame* frame, float3 origin)
{
    Seasons seasons = frame->seasons;
    float early = (windNoise(origin.xz / seasons.patchSize) - 0.5) * seasons.variation;
    float time = seasons.time + early;
    time -= 4.0 * floor(time / 4.0);
    uint from = min(uint(time), 3u);
    uint to = (from + 1) % 4;
    float along = smoothstep(0.0, 1.0, time - float(from));
    PlantSeason season;
    season.color = lerp(seasons.colors[from], seasons.colors[to], along);
    season.snow = lerp(seasons.looks[from].y, seasons.looks[to].y, along);
    return season;
}

float seasonLuminance(float3 color)
{
    return dot(color, float3(0.2126, 0.7152, 0.0722));
}

// A plant's colour and roughness through the year, and the snow and
// wetness of everything else as far as the ground takes them: `albedo` the
// material's own, `normal` the surface's facing, for snow.
void seasonalSurface(Frame* frame, Material material, float3 origin, float3 normal, inout float3 albedo, inout float roughness)
{
    Seasons seasons = frame->seasons;
    if (seasons.enabled == 0)
    {
        return;
    }
    PlantSeason season = plantSeason(frame, origin);
    // What is not a plant takes as much of the snow and wetness as the ground does.
    float cover = isPlant(material) ? 1.0 : seasons.ground;
    if (cover <= 0.0)
    {
        return;
    }
    if (isLeaves(material))
    {
        // The texture's light and dark, so a season's colour keeps the leaf's veins and edges.
        float detail = min(seasonLuminance(albedo) / max(seasonLuminance(material.baseColor.rgb), 1e-3), 2.0);
        float2 patch = origin.xz / seasons.patchSize;
        if ((material.flags & MATERIAL_LEAVES_STAY) == 0)
        {
            // Some plants redder, some yellower, as a wood in autumn.
            float warm = (windNoise(patch * 4.1 + 53.0) - 0.5) * seasons.variation;
            float3 tint = season.color.rgb / max(season.color.a, 1e-4) * float3(1.0 + warm * 1.2, 1.0 - warm * 1.8, 1.0 - warm * 2.0);
            albedo = lerp(albedo, tint * detail, season.color.a);
        }
        float dry = seasons.dry.a * smoothstep(0.35, 0.65, windNoise(patch * 1.7 + 31.0));
        albedo = lerp(albedo, seasons.dry.rgb * detail, dry);
        albedo *= 1.0 + (windNoise(patch * 5.3 + 7.0) - 0.5) * 0.4 * seasons.variation;
    }
    albedo *= 1.0 - 0.45 * seasons.wetness * cover;
    roughness *= 1.0 - 0.6 * seasons.wetness * cover;
    float snow = season.snow * cover * smoothstep(0.1, 0.6, normal.y);
    albedo = lerp(albedo, float3(0.85, 0.87, 0.9), snow);
    roughness = lerp(roughness, 0.6, snow);
}
