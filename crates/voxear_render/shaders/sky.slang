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

// ---------------------------------------------------------------- Night

// The moon and the stars are drawn as an eye adapted to the night sees them, at a brightness on screen of their
// own, whatever the exposure: the moon bright, its seas still showing, the brightest stars a little dimmer.
static const float MOON_ON_SCREEN = 0.8;
static const float STARS_ON_SCREEN = 0.7;
// The cosine of the moon's radius as drawn: two degrees across, larger than
// life, as games draw it, so it reads at a glance.
static const float MOON_COS_RADIUS = 0.99985;
// Stars: the grid round the sky they are scattered in, the share of its
// cells with a star, and how bright the brightest is, as light: most show
// against a moonlit sky, the brightest first at dusk, none against a day sky.
static const float STAR_CELLS = 160.0;
static const float STAR_SHARE = 0.04;
static const float STAR_BRIGHTNESS = 3e-4;
// The faintest a point can be against the sky round it and still be seen.
static const float STAR_CONTRAST = 0.03;

// What the composite multiplies the scene by to expose it, last frame's, so
// the sky can draw things at a brightness on screen.
float exposureScale(Frame* frame)
{
    Post post = frame->post;
    float ev = post.autoExposure != 0 ? buffers()->exposure[0] : post.exposureCompensation;
    return frame->exposure * exp2(ev);
}

float3 hash33(float3 p)
{
    uint3 v = uint3(int3(p) + 32768);
    v = v * 1664525u + 1013904223u;
    v.x += v.y * v.z;
    v.y += v.z * v.x;
    v.z += v.x * v.y;
    v ^= v >> 16u;
    v.x += v.y * v.z;
    v.y += v.z * v.x;
    v.z += v.x * v.y;
    return float3(v & 0xFFFFu) / 65535.0;
}

// Smooth noise over the plane, from 0 to 1.
float noise2(float2 p)
{
    float2 cell = floor(p);
    float2 f = p - cell;
    f = f * f * (3.0 - 2.0 * f);
    float a = hash33(float3(cell, 0.0)).x;
    float b = hash33(float3(cell + float2(1.0, 0.0), 0.0)).x;
    float c = hash33(float3(cell + float2(0.0, 1.0), 0.0)).x;
    float d = hash33(float3(cell + float2(1.0, 1.0), 0.0)).x;
    return lerp(lerp(a, b, f.x), lerp(c, d, f.x), f.y);
}

// The moon's disc towards `toMoon`, tinted by its light: pale grey, its
// seas darker, a little dimmer at its edge, as bright on screen as the eye
// adapted to the night sees it.
float3 moonDisc(Frame* frame, float3 direction, float3 toMoon)
{
    float facing = dot(direction, toMoon);
    if (facing < MOON_COS_RADIUS - 1e-5)
    {
        return 0.0;
    }
    float3 right = normalize(cross(toMoon, abs(toMoon.y) < 0.99 ? float3(0.0, 1.0, 0.0) : float3(1.0, 0.0, 0.0)));
    float3 up = cross(right, toMoon);
    float radius = sqrt(1.0 - MOON_COS_RADIUS * MOON_COS_RADIUS);
    float2 at = float2(dot(direction, right), dot(direction, up)) / radius;
    float out = length(at);
    float edge = saturate((1.0 - out) * 24.0);
    float seas = 0.55 + 0.45 * smoothstep(0.35, 0.65, noise2(at * 2.2 + 5.0) * 0.7 + noise2(at * 6.0 + 1.0) * 0.3);
    float limb = 0.8 + 0.2 * sqrt(saturate(1.0 - out * out));
    float3 tint = frame->sunColor / max(max(frame->sunColor.r, max(frame->sunColor.g, frame->sunColor.b)), 1e-6);
    return tint * MOON_ON_SCREEN / exposureScale(frame) * seas * limb * edge;
}

// The stars along `direction`, each about `pixel` across (the pixel's own
// angle), so they neither vanish nor shimmer as the view turns. Most are
// faint; a few bright, and warmer or bluer than white. They are always
// there: a day sky is so much brighter that they do not show.
float3 starField(float3 direction, float pixel)
{
    float3 around = direction * STAR_CELLS - 0.5;
    float3 base = floor(around);
    float3 light = 0.0;
    // A star's own cell or one beside it may hold the star nearest this pixel.
    for (uint corner = 0; corner < 8; corner++)
    {
        float3 cell = base + float3(corner & 1, (corner >> 1) & 1, (corner >> 2) & 1);
        float3 random = hash33(cell);
        if (random.x > STAR_SHARE)
        {
            continue;
        }
        float3 star = normalize(cell + 0.2 + 0.6 * hash33(cell + 101.0));
        // Each star belongs to the cell it lies in, so none is drawn twice.
        if (any(floor(star * STAR_CELLS) != cell))
        {
            continue;
        }
        float apart = length(direction - star) / max(pixel, 1e-5);
        float brightness = exp2(-10.0 * random.y);
        float3 colour = lerp(float3(1.0, 0.9, 0.8), float3(0.85, 0.9, 1.0), random.z);
        light += colour * brightness * exp(-apart * apart * 2.5);
    }
    return light * STAR_BRIGHTNESS;
}

// ---------------------------------------------------------------- The sky

float3 gradientSky(Frame* frame, float3 direction)
{
    float height = direction.y;
    float3 sky = lerp(frame->skyHorizon, frame->skyZenith, pow(saturate(height), 0.45));
    float3 ground = lerp(frame->skyHorizon, frame->groundColor, pow(saturate(-height), 0.35));
    float3 color = height >= 0.0 ? sky : ground;
    float toSun = dot(direction, -frame->sunDirection);
    if (frame->lighting.moon != 0)
    {
        return color + moonDisc(frame, direction, -frame->sunDirection);
    }
    return color + frame->sunColor * (smoothstep(0.99955, 0.9998, toSun) * 8.0 + pow(saturate(toSun), 48.0) * 0.12);
}

float3 atmosphereSky(Frame* frame, float3 direction)
{
    Atmosphere atmosphere = frame->lighting.atmosphere;
    float3 color = skyToward(frame, direction) * skyLook(frame);
    float3 origin = cameraInAtmosphere(frame);
    float height = length(origin);
    float toSun = dot(direction, towardsSun(frame));
    if (frame->lighting.moon != 0)
    {
        if (raySphere(origin, direction, atmosphere.bottomRadius) < 0.0)
        {
            float3 through = transmittanceToSpace(frame->lighting, height, dot(direction, origin / height));
            color += moonDisc(frame, direction, towardsSun(frame)) * through;
        }
        return color;
    }
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
    bool atmosphere = frame->lighting.sky == SKY_ATMOSPHERE;
    float3 color = atmosphere ? atmosphereSky(frame, direction) : gradientSky(frame, direction);
    if (frame->stars > 0.0 && direction.y > 0.0)
    {
        // Dimmed through the air as everything beyond it is: faint near the horizon.
        float3 origin = cameraInAtmosphere(frame);
        float height = length(origin);
        float3 through = atmosphere ? transmittanceToSpace(frame->lighting, height, dot(direction, origin / height))
            : saturate(direction.y * 6.0);
        // The eye cannot pick out a point much fainter than a few hundredths of the sky round it, which is
        // why stars go by day: each shows only as far as it outshines that, and then as the eye sees it.
        float3 stars = starField(direction, length(fwidth(direction))) * through;
        float3 seen = saturate((stars - color * STAR_CONTRAST) / max(stars, 1e-9));
        color += stars / STAR_BRIGHTNESS * seen * STARS_ON_SCREEN / exposureScale(frame) * frame->stars;
    }
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
