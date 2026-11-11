// Lava: plates of dark crust drifting on molten rock, which glows through
// the cracks between them. Made after how lava cools: the cracks glow
// hottest, the crust beside them faintly, and the colour follows the
// molten rock's temperature as a real glow does, from dull red to yellow.
//
// - Plates: cells of a Voronoi pattern, cracked along their borders (Quilez,
//   "Voronoi edges"), with finer cracks across them, drifting with the flow
//   and churning as they go.
// - The glow seen a little way down in the cracks, under the crust's edge.
// - Plates raised at their middles and bevelled down to the cracks, glassy
//   beside them and rough and dull on top.
//
// Lava lights nothing round it: put a Point Light near for that, orange.

struct Lava
{
    [Property, Range(0.2, 10), Tooltip("How wide the plates of crust are, in metres")] float plateSize = 1.5;
    [Property, Range(0.005, 0.5), Tooltip("How wide the glowing cracks between the plates are, in metres")] float crackWidth = 0.06;
    [Property, Range(800, 2000), Tooltip("How hot the molten rock is, in kelvin. 1000 glows dull red, 1300 orange, 1800 yellow")] float temperature = 1250.0;
    [Property, Range(0, 100), Tooltip("How brightly the molten rock glows")] float glow = 12.0;
    [Property, Tooltip("Which way and how fast it flows across the ground, in metres a second")] float2 flow = float2(0.08, 0.02);
    [Property, Range(0, 1), Tooltip("How much the plates drift and churn as they go")] float churn = 0.4;
    [Property, Range(0, 1), Tooltip("How much the glow throbs")] float pulse = 0.3;
    [Property, Color] float3 crust = float3(0.08, 0.065, 0.06);
    [Property, Tooltip("A picture of the crust's rock, laid on by its UVs")] Picture crustPicture;
}

float2 hash2(float2 p)
{
    p = float2(dot(p, float2(127.1, 311.7)), dot(p, float2(269.5, 183.3)));
    return frac(sin(p) * 43758.5453);
}

float noise(float2 p)
{
    float2 i = floor(p);
    float2 f = frac(p);
    float2 u = f * f * (3.0 - 2.0 * f);
    float a = hash2(i).x;
    float b = hash2(i + float2(1, 0)).x;
    float c = hash2(i + float2(0, 1)).x;
    float d = hash2(i + float2(1, 1)).x;
    return lerp(lerp(a, b, u.x), lerp(c, d, u.x), u.y);
}

float layered(float2 p)
{
    return noise(p) * 0.57 + noise(p * 2.03 + 3.1) * 0.29 + noise(p * 4.01 + 7.7) * 0.14;
}

// Where a cell's point is, moving slowly round its middle as the plates churn.
float2 seed(float2 cell, float time, float churn)
{
    float2 at = 0.15 + 0.7 * hash2(cell);
    return at + 0.12 * churn * sin(time * 0.15 + 6.2831 * at.yx);
}

// The distance to the nearest border between cells, in cells, and the way
// to it: the exact distance, so cracks are as wide everywhere (Quilez).
float3 cellEdge(float2 x, float time, float churn)
{
    float2 n = floor(x);
    float2 f = frac(x);
    float2 nearest = 0.0;
    float2 nearestCell = 0.0;
    float closest = 8.0;
    for (int j = -1; j <= 1; j++)
    {
        for (int i = -1; i <= 1; i++)
        {
            float2 g = float2(i, j);
            float2 r = g + seed(n + g, time, churn) - f;
            float d = dot(r, r);
            if (d < closest)
            {
                closest = d;
                nearest = r;
                nearestCell = g;
            }
        }
    }
    float edge = 8.0;
    float2 toEdge = float2(1.0, 0.0);
    for (int j = -2; j <= 2; j++)
    {
        for (int i = -2; i <= 2; i++)
        {
            float2 g = nearestCell + float2(i, j);
            float2 r = g + seed(n + g, time, churn) - f;
            float2 across = r - nearest;
            if (dot(across, across) > 1e-5)
            {
                float2 way = normalize(across);
                float d = dot(0.5 * (nearest + r), way);
                if (d < edge)
                {
                    edge = d;
                    toEdge = way;
                }
            }
        }
    }
    return float3(edge, toEdge);
}

// The colour of a glow at `kelvin`, by Mitchell Charity's blackbody table:
// red, with green rising as it heats, and no blue below white heat.
float3 blackbody(float kelvin)
{
    float t = kelvin / 1000.0;
    return float3(1.0, 0.04 * t * t * t, 0.05 * saturate((kelvin - 1600.0) / 400.0));
}

void surface(Lava lava, SurfaceInput input, inout SurfaceOutput surface)
{
    float time = input.time;
    float size = lava.plateSize;
    float fineSize = size * 0.3;
    // Bent by slow noise, so the plates are rounded and uneven rather than tiles.
    float2 flowing = input.levelPosition.xz - lava.flow * time;
    float2 p = flowing + (float2(layered(flowing / size * 0.4), layered(flowing / size * 0.4 + 9.3)) - 0.5) * size * 0.9;
    float3 plate = cellEdge(p / size, time, lava.churn);
    float3 fine = cellEdge(p / fineSize + 17.0, time, lava.churn);
    // Only some plates have cracked across.
    float cracked = smoothstep(0.45, 0.6, layered(p / size * 0.6 + 5.0));
    float fineEdge = lerp(8.0, fine.x * fineSize, cracked);

    // The cracks, wider in some places than others; the finer ones narrower.
    float width = lava.crackWidth * (0.6 + 0.8 * layered(p * 0.7));
    float crusted = smoothstep(width, width + 0.02, plate.x * size) * smoothstep(width * 0.35, width * 0.35 + 0.01, fineEdge);

    // The glow seen a little way down the cracks, under the crust's edge.
    float3 v = input.view;
    float2 sunk = p - v.xz / max(v.y, 0.2) * 0.03;
    float sunkFine = lerp(8.0, cellEdge(sunk / fineSize + 17.0, time, lava.churn).x * fineSize, cracked);
    float sunkEdge = min(cellEdge(sunk / size, time, lava.churn).x * size, sunkFine * 3.0);
    float slow = layered(p * 0.3 + time * 0.05);
    float heat = saturate(exp(-max(sunkEdge - width, 0.0) / 0.04) + 0.3 * (slow - 0.5));
    float beat = 1.0 + lava.pulse * 0.35 * sin(time * 1.3 + 6.2831 * slow);
    float kelvin = lerp(750.0, lava.temperature, heat);
    float strength = lava.glow * pow(heat, 4.0) * pow(lava.temperature / 1250.0, 4.0) * beat;
    // Below about 800 kelvin rock gives off no light to see.
    float visible = smoothstep(750.0, 850.0, kelvin);
    surface.emission = blackbody(kelvin) * strength * visible * lerp(1.0, 0.12, crusted);

    // Crust raised at its middle and bevelled down to the cracks, glassy beside them, dull on top.
    float bevel = saturate((plate.x * size - width) / 0.12);
    float rise = 0.02 * 6.0 * bevel * (1.0 - bevel) / 0.12;
    float3 tilt = float3(plate.y * rise, 0.0, plate.z * rise) * crusted;
    // And rough, lumpy rock on top.
    float2 bump = float2(layered(p * 6.0) - layered(p * 6.0 + float2(0.05, 0.0)), layered(p * 6.0) - layered(p * 6.0 + float2(0.0, 0.05)));
    tilt.xz += bump * 2.0 * crusted;
    surface.normal = normalize(input.normal + tilt);
    // The crust's rock darker and lighter from plate to plate, and from place to place on each.
    float shade = 0.7 + 0.6 * layered(p * 1.7 + 11.0);
    float3 rock = lava.crust * shade * lava.crustPicture.sample(input.uv).rgb;
    surface.color = lerp(blackbody(kelvin) * 0.1, rock, crusted);
    surface.metallic = 0.0;
    surface.roughness = lerp(0.25, lerp(0.5, 0.9, smoothstep(0.0, 0.2, plate.x * size - width)), crusted);
}
