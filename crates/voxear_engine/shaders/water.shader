// Water: a sea, a lake or a pond, drawn in a single layer over what is
// under it.
//
// - Waves: 24 Gerstner waves across the wind, each as steep as the next and
//   as fast as real water of its length goes; the 6 longest move the
//   vertices, every one tilts the surface. Waves too small to see from far
//   off fade out and leave their roughness behind, so the sea in the
//   distance glitters rather than shimmers.
// - Light through the thin tops of the waves, in its own colour.
// - Foam where the crests break, in lacy patches.
// - It reflects as water does, 2% head on.
// - With the material's Glass on, what lies under it shows through, the
//   deeper the bluer and darker, light lost a colour at a time as through
//   real water; foam laps at the shore; and the waves bend the view.
//
// The waves move a mesh's vertices, so give it one with many of them; on a
// single flat quad only the light shows them.

[MaxOffset(3.0)]
struct Water
{
    [Property, Color, Tooltip("The colour of deep water, where no bottom shows")] float3 deep = float3(0.0, 0.12, 0.2);
    [Property, Color, Tooltip("What a metre of it lets through, by colour. Clear sea keeps blue and green and loses red. With Glass on")] float3 clarity = float3(0.82, 0.97, 0.98);
    [Property, Range(0.1, 10), Tooltip("How murky it is. 1 as Clarity says, more to see less far into it")] float murk = 1.0;
    [Property, Color, Tooltip("The colour light takes through the thin tops of the waves")] float3 scatter = float3(0.25, 0.7, 0.63);
    [Property, Range(0, 3), Tooltip("How high the biggest waves are, in metres")] float waveHeight = 0.3;
    [Property, Range(1, 100), Tooltip("How far from one big wave to the next, in metres")] float waveLength = 10.0;
    [Property, Range(0, 3), Tooltip("How fast the waves go. 1 as fast as real water")] float waveSpeed = 1.0;
    [Property, Tooltip("Which way the wind blows the waves, across the ground")] float2 wind = float2(1.0, 0.3);
    [Property, Range(0, 1), Tooltip("How sharp the crests are. 0 for round swells")] float choppiness = 0.5;
    [Property, Range(0, 2), Tooltip("How strong the small ripples on the waves are")] float ripples = 1.0;
    [Property, Range(0, 1), Tooltip("How much foam the breaking crests have. 0 for none")] float foam = 0.4;
    [Property, Range(0, 3), Tooltip("How far out from the shore foam reaches, in metres. With Glass on")] float shoreFoam = 0.6;
    [Property, Range(0, 1), Tooltip("How smooth the water is between the ripples")] float smoothness = 0.97;
}

// How many waves move the vertices, how many tilt the surface, and how much shorter each is than the one before.
static const int BIG_WAVES = 6;
static const int ALL_WAVES = 24;
static const float SHORTER = 0.79;

struct Wave
{
    float2 way;
    float k;
    float amplitude;
    float omega;
    float phase;
    // How far it moves the water towards its crests, for each metre high.
    float q;
};

float hash(float n)
{
    return frac(sin(n * 127.1 + 31.7) * 43758.5453);
}

// Wave `i`: across the wind, spreading wider the shorter it is; its height
// growing with its length, so all are as steep; and as fast as deep water
// carries a wave of its length.
Wave wave(Water water, int i)
{
    Wave w;
    float spread = 0.35 + 0.9 * float(i) / float(ALL_WAVES);
    float angle = atan2(water.wind.y, water.wind.x) + (hash(float(i)) * 2.0 - 1.0) * spread;
    w.way = float2(cos(angle), sin(angle));
    float shrink = pow(SHORTER, float(i));
    w.k = 2.0 * PI / max(water.waveLength * shrink, 0.01);
    w.omega = sqrt(9.81 * w.k) * water.waveSpeed;
    // The ripples steeper than the swell, as wind-driven ripples on real water are.
    w.amplitude = water.waveHeight * 0.3 * shrink * (i < BIG_WAVES ? 1.0 : 2.5 * water.ripples);
    w.phase = hash(float(i) + 0.5) * 2.0 * PI;
    // Sharpened together by Choppiness at most, so the crests never loop over themselves.
    w.q = i < BIG_WAVES ? water.choppiness / max(w.k * w.amplitude * float(BIG_WAVES), 1e-4) : 0.0;
    return w;
}

float angleOf(Wave w, float2 rest, float time)
{
    return w.k * dot(w.way, rest) - fmod(w.omega * time, 2.0 * PI) + w.phase;
}

// How far the big waves move the water resting at `rest`: across, and up.
float3 displacement(Water water, float2 rest, float time)
{
    float3 moved = 0.0;
    for (int i = 0; i < BIG_WAVES; i++)
    {
        Wave w = wave(water, i);
        float theta = angleOf(w, rest, time);
        moved.xz += w.q * w.amplitude * w.way * cos(theta);
        moved.y += w.amplitude * sin(theta);
    }
    return moved;
}

float2 hash2(float2 p)
{
    p = float2(dot(p, float2(127.1, 311.7)), dot(p, float2(269.5, 183.3)));
    return frac(sin(p) * 43758.5453);
}

// The distance to the nearest of a scatter of points, a cell a unit wide: small in the middle of foam's bubbles.
float cells(float2 p)
{
    float2 n = floor(p);
    float2 f = frac(p);
    float nearest = 8.0;
    for (int j = -1; j <= 1; j++)
    {
        for (int i = -1; i <= 1; i++)
        {
            float2 g = float2(i, j);
            float2 r = g + hash2(n + g) - f;
            nearest = min(nearest, dot(r, r));
        }
    }
    return sqrt(nearest);
}

float3 vertex(Water water, VertexInput input)
{
    return displacement(water, input.position.xz, input.time);
}

void surface(Water water, SurfaceInput input, inout SurfaceOutput surface)
{
    // Where the water drawn here rests: the waves moved it across as well as up, so step back from where it is.
    float2 rest = input.position.xz;
    for (int n = 0; n < 2; n++)
    {
        rest = input.position.xz - displacement(water, rest, input.time).xz;
    }

    // Every wave's tilt, and how the big ones squeeze the water towards their crests, where it breaks.
    float footprint = max(length(fwidth(input.position.xz)), 1e-4);
    float3 normal = float3(0.0, 1.0, 0.0);
    float height = 0.0;
    float lost = 0.0;
    float3 squeeze = float3(1.0, 1.0, 0.0);
    for (int i = 0; i < ALL_WAVES; i++)
    {
        Wave w = wave(water, i);
        float steep = w.k * w.amplitude;
        // Gone once shorter than four pixels, its slopes left as roughness.
        float fade = saturate((2.0 * PI / w.k) / (4.0 * footprint) - 1.0);
        lost += (1.0 - fade * fade) * steep * steep * 0.5;
        if (fade <= 0.0)
        {
            continue;
        }
        float theta = angleOf(w, rest, input.time);
        float c = cos(theta);
        float s = sin(theta);
        float tilt = steep * fade;
        normal.xz -= w.way * tilt * c;
        normal.y -= w.q * tilt * s;
        squeeze -= w.q * tilt * s * float3(w.way.x * w.way.x, w.way.y * w.way.y, w.way.x * w.way.y);
        if (i < BIG_WAVES)
        {
            height += w.amplitude * s;
        }
    }
    normal = normalize(normal);

    // Foam where the water folds onto itself at a crest, in bubbles that drift with the wind.
    float folded = squeeze.x * squeeze.y - squeeze.z * squeeze.z;
    float breaking = water.foam > 0.0 ? saturate((lerp(0.3, 1.0, water.foam) - folded) / 0.25) : 0.0;
    float2 drift = normalize(water.wind + float2(1e-4, 0.0)) * input.time * 0.15;
    float lace = cells(rest * 1.8 + drift) * 0.75 + cells(rest * 5.3 - drift) * 0.25;
    float foam = saturate((breaking * 1.4 - lace) * 4.0);

    float3 deep = water.deep;
    if (input.glass)
    {
        // Light lost a colour at a time as it goes through: red first, so the deeper the bluer and darker.
        float3 absorbs = -log(max(water.clarity, float3(1e-3))) * water.murk;
        float3 through = exp(-absorbs * input.thickness);
        // Foam laps at the shore, in bands that wash in.
        float shore = water.shoreFoam > 0.0 ? saturate(1.0 - input.thickness / water.shoreFoam) : 0.0;
        float wash = 0.6 + 0.4 * sin(input.thickness / max(water.shoreFoam, 0.01) * 6.0 - input.time * 1.5);
        foam = max(foam, saturate((shore * wash * 1.3 - lace) * 4.0));
        surface.transmission = through * (1.0 - foam);
        // The water itself scatters what it takes in, more the deeper it is.
        deep *= 1.0 - through;
    }

    // As rough as the waves too small to draw would make it.
    float alpha = (1.0 - water.smoothness) * (1.0 - water.smoothness);
    float rough = sqrt(sqrt(alpha * alpha + 2.0 * lost));
    surface.color = lerp(deep, float3(0.85), foam);
    surface.normal = normalize(lerp(normalize(input.normal + float3(normal.x, normal.y - 1.0, normal.z)), input.normal, foam * 0.6));
    surface.metallic = 0.0;
    surface.roughness = lerp(rough, 0.7, foam);
    surface.reflectance = lerp(0.02, 0.04, foam);
    // For light: how high on a crest it is, and how much foam.
    surface.data = float4(saturate(height / max(water.waveHeight, 1e-3)), foam, 0.0, 0.0);
}

// The sun and lamps on water: their reflection, sharp and 2% head on; the
// foam and murk they light; and light through the thin tops of the waves
// between the viewer and the light, as Atlas lights its sea.
float3 light(Water water, LightInput light, SurfaceOutput surface)
{
    float3 n = surface.normal;
    float3 l = light.direction;
    float3 v = light.view;
    float3 h = normalize(l + v);
    float nl = saturate(dot(n, l));
    float nv = max(dot(n, v), 1e-4);
    float nh = saturate(dot(n, h));
    float a = max(surface.roughness * surface.roughness, 0.002);
    float a2 = a * a;
    float spread = nh * nh * (a2 - 1.0) + 1.0;
    float distribution = a2 / (PI * spread * spread);
    float visibility = 0.5 / max(nl * sqrt(nv * nv * (1.0 - a2) + a2) + nv * sqrt(nl * nl * (1.0 - a2) + a2), 1e-5);
    float fresnel = surface.reflectance + (1.0 - surface.reflectance) * pow(1.0 - saturate(dot(v, h)), 5.0);
    float specular = distribution * visibility * fresnel * nl;

    float crest = surface.data.x;
    float foam = surface.data.y;
    float towards = pow(saturate(dot(l, -v)), 4.0) * pow(0.5 - 0.5 * dot(l, n), 3.0);
    float glow = 2.0 * crest * towards + 0.08 * pow(saturate(dot(v, n)), 2.0) + 0.04 * nl;
    float3 through = water.scatter * glow * (1.0 - foam);
    return light.color * (specular + surface.color / PI * nl + through);
}
