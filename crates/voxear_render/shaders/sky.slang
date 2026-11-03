// The sky drawn behind everything: only pixels still at the far plane
// (depth 0 with reverse-Z) pass the depth test. A scene's sky is either a
// gradient of its own colours or the atmosphere (atmosphere.slang), whose
// lookup tables the fragment shaders after the sky's draw before the scene.

#include "atmosphere.slang"

struct SkyOut
{
    float4 position : SV_Position;
    float2 ndc : TEXCOORD0;
};

[shader("vertex")]
SkyOut skyVertex(uint vertexId : SV_VertexID)
{
    SkyOut o;
    o.ndc = fullscreenNdc(vertexId);
    o.position = float4(o.ndc, 0.0, 1.0);
    return o;
}

struct SkyFragmentOut
{
    float4 color : SV_Target0;
    float2 velocity : SV_Target1;
};

// The sky is infinitely far away, so only camera rotation moves it.
float2 skyMotion(Frame* frame, float3 direction)
{
    float4 current = transform(frame->unjitteredViewProj, float4(direction, 0.0));
    float4 previous = transform(frame->prevViewProj, float4(direction, 0.0));
    return (current.xy / current.w - previous.xy / previous.w) * 0.5;
}

float3 gradientSky(Frame* frame, float3 direction)
{
    float height = direction.y;
    float3 sky = lerp(frame->skyHorizon, frame->skyZenith, pow(saturate(height), 0.45));
    float3 ground = lerp(frame->skyHorizon, frame->groundColor, pow(saturate(-height), 0.35));
    float3 color = height >= 0.0 ? sky : ground;
    float toSun = dot(direction, -frame->sunDirection);
    return color + frame->sunColor * (smoothstep(0.99955, 0.9998, toSun) * 8.0 + pow(saturate(toSun), 48.0) * 0.12);
}

float3 atmosphereSky(Frame* frame, float3 direction)
{
    Atmosphere atmosphere = frame->lighting.atmosphere;
    float3 color = skyToward(frame, direction) * frame->sunColor;
    float3 origin = cameraInAtmosphere(frame);
    float height = length(origin);
    float toSun = dot(direction, towardsSun(frame));
    if (toSun > atmosphere.sunCosRadius - 1e-5 && raySphere(origin, direction, atmosphere.bottomRadius) < 0.0)
    {
        // Antialiased over the disc's last tenth, and dimmed through the air as its light is.
        float edge = saturate((toSun - atmosphere.sunCosRadius) / max((1.0 - atmosphere.sunCosRadius) * 0.2, 1e-7));
        float3 through = transmittanceToSpace(frame->lighting, height, dot(direction, origin / height));
        color += frame->sunColor * through * SUN_DISC_BRIGHTNESS * edge;
    }
    return color;
}

[shader("fragment")]
SkyFragmentOut skyFragment(SkyOut input)
{
    Frame* frame = push.frame;
    float4 near = transform(frame->invViewProj, float4(input.ndc, 1.0, 1.0));
    float3 direction = normalize(near.xyz / near.w - frame->cameraPosition);
    SkyFragmentOut o;
    float3 color = frame->lighting.sky == SKY_ATMOSPHERE ? atmosphereSky(frame, direction) : gradientSky(frame, direction);
    o.color = float4(color, 1.0);
    o.velocity = skyMotion(frame, direction);
    return o;
}

// The atmosphere's tables, each a fullscreen draw into its own image.

struct LutOut
{
    float4 position : SV_Position;
    float2 uv : TEXCOORD0;
};

[shader("vertex")]
LutOut lutVertex(uint vertexId : SV_VertexID)
{
    LutOut o;
    float2 ndc = fullscreenNdc(vertexId);
    o.position = float4(ndc, 0.0, 1.0);
    o.uv = ndc * 0.5 + 0.5;
    return o;
}

[shader("fragment")]
float4 transmittanceFragment(LutOut input) : SV_Target0
{
    Atmosphere atmosphere = push.frame->lighting.atmosphere;
    float height;
    float cosZenith;
    transmittanceParameters(atmosphere, input.position.xy / TRANSMITTANCE_SIZE, height, cosZenith);
    float3 origin = float3(0.0, height, 0.0);
    float3 direction = float3(sqrt(max(1.0 - cosZenith * cosZenith, 0.0)), cosZenith, 0.0);
    float end = raySphere(origin, direction, atmosphere.topRadius);
    const uint STEPS = 40;
    float3 depth = 0.0;
    for (uint i = 0; i < STEPS; i++)
    {
        float t = end * (float(i) + 0.5) / float(STEPS);
        depth += mediumAt(atmosphere, origin + direction * t).extinction * (end / float(STEPS));
    }
    return float4(exp(-depth), 1.0);
}

// Each texel gathers the light scattered once towards it from every
// direction (64 of them), and the share of light scattered again; the sum
// of every order after is then a geometric series (Hillaire 2020, 5.5).
[shader("fragment")]
float4 multiScatteringFragment(LutOut input) : SV_Target0
{
    Frame* frame = push.frame;
    Atmosphere atmosphere = frame->lighting.atmosphere;
    float2 uv = fromSubUvsToUnit(input.position.xy / MULTI_SCATTERING_SIZE, MULTI_SCATTERING_SIZE);
    float cosSun = uv.x * 2.0 - 1.0;
    float height = atmosphere.bottomRadius + saturate(uv.y) * (atmosphere.topRadius - atmosphere.bottomRadius - PLANET_OFFSET)
        + PLANET_OFFSET;
    float3 origin = float3(0.0, height, 0.0);
    // The table's own sun, by its angle from straight up.
    float3 sun = float3(sqrt(max(1.0 - cosSun * cosSun, 0.0)), cosSun, 0.0);

    const uint SIDE = 8;
    float3 luminance = 0.0;
    float3 share = 0.0;
    for (uint j = 0; j < SIDE; j++)
    {
        for (uint i = 0; i < SIDE; i++)
        {
            float around = 2.0 * PI * (float(i) + 0.5) / float(SIDE);
            float cosUp = 1.0 - 2.0 * (float(j) + 0.5) / float(SIDE);
            float sinUp = sqrt(max(1.0 - cosUp * cosUp, 0.0));
            float3 direction = float3(cos(around) * sinUp, cosUp, sin(around) * sinUp);
            Scattered scattered = integrate(frame->lighting, sun, origin, direction, 1e9, 20, true, true, false);
            luminance += scattered.luminance;
            share += scattered.multiScatteringShare;
        }
    }
    // Over the sphere's 4π, by the isotropic phase's 1/4π: the two cancel.
    float count = float(SIDE * SIDE);
    luminance /= count;
    share /= count;
    return float4(luminance / max(1.0 - share, 1e-4), 1.0);
}

[shader("fragment")]
float4 skyViewFragment(LutOut input) : SV_Target0
{
    Frame* frame = push.frame;
    Atmosphere atmosphere = frame->lighting.atmosphere;
    float3 camera = cameraInAtmosphere(frame);
    float height = length(camera);
    float cosZenith;
    float cosSunAround;
    skyViewParameters(atmosphere, height, input.position.xy / SKY_VIEW_SIZE, cosZenith, cosSunAround);

    // In a frame of the camera's own: up, with the sun round in the XY plane.
    float3 up = float3(0.0, 1.0, 0.0);
    float cosSun = dot(towardsSun(frame), camera / height);
    float3 sun = float3(sqrt(max(1.0 - cosSun * cosSun, 0.0)), cosSun, 0.0);
    float sinZenith = sqrt(max(1.0 - cosZenith * cosZenith, 0.0));
    float sinSunAround = sqrt(max(1.0 - cosSunAround * cosSunAround, 0.0));
    float3 direction = float3(sinZenith * cosSunAround, cosZenith, sinZenith * sinSunAround);
    Scattered scattered = integrate(frame->lighting, sun, up * height, direction, 1e9, 30, true, false, true);
    return float4(scattered.luminance, 1.0);
}

// Each slice's froxels hold what the air adds between the camera and the
// slice's depth, and the share of light it lets through, as alpha's
// complement. Slices are spaced by the square of their number, so more lie
// close by.
[shader("fragment")]
float4 aerialPerspectiveFragment(LutOut input) : SV_Target0
{
    Frame* frame = push.frame;
    uint2 pixel = uint2(input.position.xy);
    uint slice = pixel.x / uint(AERIAL_SLICES);
    float2 inSlice = (float2(pixel.x % uint(AERIAL_SLICES), pixel.y) + 0.5) / AERIAL_SLICES;
    float2 ndc = inSlice * 2.0 - 1.0;
    float4 near = transform(frame->invViewProj, float4(ndc, 1.0, 1.0));
    float3 direction = normalize(near.xyz / near.w - frame->cameraPosition);
    float depth = (float(slice) + 0.5) / AERIAL_SLICES;
    float kilometres = depth * depth * AERIAL_SLICES * AERIAL_KM_PER_SLICE;
    uint steps = clamp((slice + 1) * 2, 2u, 64u);
    Scattered scattered = integrate(frame->lighting, towardsSun(frame), cameraInAtmosphere(frame), direction, kilometres, steps, false, false, true);
    float transmittance = dot(scattered.transmittance, float3(1.0 / 3.0));
    return float4(scattered.luminance, 1.0 - transmittance);
}
