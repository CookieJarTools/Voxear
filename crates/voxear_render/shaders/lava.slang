// Lava's light and the shapes of its crust, shared by lava Pools and Streams (water.slang) and terrain surfaces of
// lava (terrain.slang).

#pragma once

// The light of something as hot as `kelvin`, by Planck's law at red's,
// green's and blue's wavelengths: 1 in red at 1400 K, where it is orange;
// yellow and far brighter hotter; dull red, and dimming fast, as it cools.
float3 blackbody(float kelvin)
{
    static const float3 wavelength = float3(610e-9, 550e-9, 465e-9);
    float3 shorter = pow(wavelength / 610e-9, 5.0);
    float3 planck = 1.0 / (shorter * (exp(1.4388e-2 / (wavelength * max(kelvin, 300.0))) - 1.0));
    return planck * (exp(1.4388e-2 / (610e-9 * 1400.0)) - 1.0);
}

float2 cellPoint(float2 cell)
{
    return frac(sin(float2(dot(cell, float2(127.1, 311.7)), dot(cell, float2(269.5, 183.3)))) * 43758.5453);
}

// How far `p` is from the nearest edge between a scatter of cells a unit
// wide (x), and which cell it is in, from 0 to 1 (y).
float2 cellEdge(float2 p)
{
    float2 n = floor(p);
    float2 f = frac(p);
    float2 nearest = 0.0;
    float2 nearestCell = 0.0;
    float best = 8.0;
    for (int j = -1; j <= 1; j++)
    {
        for (int i = -1; i <= 1; i++)
        {
            float2 g = float2(i, j);
            float2 r = g + cellPoint(n + g) - f;
            float d = dot(r, r);
            if (d < best)
            {
                best = d;
                nearest = r;
                nearestCell = n + g;
            }
        }
    }
    // Half way to each neighbour, across the line between them.
    float edge = 8.0;
    for (int j = -2; j <= 2; j++)
    {
        for (int i = -2; i <= 2; i++)
        {
            float2 g = nearestCell - n + float2(i, j);
            float2 r = g + cellPoint(n + g) - f;
            float2 between = r - nearest;
            if (dot(between, between) > 1e-5)
            {
                edge = min(edge, dot(0.5 * (nearest + r), normalize(between)));
            }
        }
    }
    return float2(edge, cellPoint(nearestCell).x);
}

// Smooth noise from 0 to 1, a unit across.
float lavaNoise(float2 p)
{
    float2 n = floor(p);
    float2 f = frac(p);
    f = f * f * (3.0 - 2.0 * f);
    float a = cellPoint(n).x;
    float b = cellPoint(n + float2(1, 0)).x;
    float c = cellPoint(n + float2(0, 1)).x;
    float d = cellPoint(n + float2(1, 1)).x;
    return lerp(lerp(a, b, f.x), lerp(c, d, f.x), f.y);
}
