// A physically based sky: the planet's atmosphere as Hillaire models it in
// "A Scalable and Production Ready Sky and Atmosphere Rendering Technique"
// (2020). Air scatters blue light (Rayleigh), aerosols scatter all colours
// forward (Mie), and ozone absorbs some red and green.
//
// Four lookup tables are drawn each frame, in sky.slang, before the scene:
// - transmittance from any height in any direction to space, 256×64;
// - light scattered many times, by height and sun angle, 32×32, so the sky
//   stays bright and blue, not dark, with the sun low;
// - the sky seen from the camera, by direction, 192×108, finer near the
//   horizon, which the sky pass samples;
// - aerial perspective: what the air between the camera and each distance
//   adds and takes away, over 32 slices of 32×32 froxels laid side by side,
//   which the forward pass lays over distant geometry.
// Everything here is in kilometres, with the planet's centre below the
// world's origin, so the scene's ground at y = 0 is the planet's surface.

#pragma once

#include "common.slang"

static const float2 TRANSMITTANCE_SIZE = float2(256.0, 64.0);
static const float2 MULTI_SCATTERING_SIZE = float2(32.0, 32.0);
static const float2 SKY_VIEW_SIZE = float2(192.0, 108.0);
static const float AERIAL_SLICES = 32.0;
static const float AERIAL_KM_PER_SLICE = 4.0;
// Points are kept this far above the ground, so rays from them do not hit it at once.
static const float PLANET_OFFSET = 0.01;
// How much brighter the sun's disc is than the light it gives, to show as the sun without flooding bloom.
static const float SUN_DISC_BRIGHTNESS = 20.0;
// How bright the sky looks under the moon, against the moon's light: games make moonlight bright enough to see
// the scene by, far brighter than it is, and a sky lit in proportion would look like a dim day.
static const float MOON_SKY = 0.04;

float2 fromUnitToSubUvs(float2 uv, float2 size)
{
    return (uv + 0.5 / size) * (size / (size + 1.0));
}

float2 fromSubUvsToUnit(float2 uv, float2 size)
{
    return (uv - 0.5 / size) * (size / (size - 1.0));
}

// The distance along a ray to a sphere round the planet's centre: the near
// hit, or the far one from inside; -1 when it misses or is behind.
float raySphere(float3 origin, float3 direction, float radius)
{
    float b = dot(origin, direction);
    float c = dot(origin, origin) - radius * radius;
    float discriminant = b * b - c;
    if (discriminant < 0.0)
    {
        return -1.0;
    }
    float root = sqrt(discriminant);
    float near = -b - root;
    float far = -b + root;
    if (near >= 0.0)
    {
        return near;
    }
    return far >= 0.0 ? far : -1.0;
}

// Where the camera is relative to the planet's centre, in kilometres: as high as it is over the level's ground,
// wherever the world's middle has moved to.
float3 cameraInAtmosphere(Frame* frame)
{
    Atmosphere atmosphere = frame->lighting.atmosphere;
    float3 camera = frame->cameraPosition;
    float3 position = float3(camera.x, levelHeight(frame, camera), camera.z) * 0.001;
    position += float3(0.0, atmosphere.bottomRadius, 0.0);
    float height = length(position);
    float lowest = atmosphere.bottomRadius + PLANET_OFFSET;
    return height < lowest ? position * (lowest / height) : position;
}

// The direction towards the sun.
float3 towardsSun(Frame* frame)
{
    return -frame->sunDirection;
}

// The light the sky is drawn with, as it looks rather than as it lights the
// scene: the sun's, or under the moon a night sky's, far darker.
float3 skyLook(Frame* frame)
{
    return frame->sunColor * (frame->lighting.moon != 0 ? MOON_SKY : 1.0);
}

struct Medium
{
    float3 rayleigh;
    float3 mie;
    float3 scattering;
    float3 extinction;
};

Medium mediumAt(Atmosphere atmosphere, float3 position)
{
    float height = max(length(position) - atmosphere.bottomRadius, 0.0);
    float rayleighDensity = exp(-height / atmosphere.rayleighHeight);
    float mieDensity = exp(-height / atmosphere.mieHeight);
    // Ozone lies in a layer peaking 25 km up.
    float ozoneDensity = max(0.0, 1.0 - abs(height - 25.0) / 15.0);
    Medium medium;
    medium.rayleigh = atmosphere.rayleighScattering * rayleighDensity;
    medium.mie = atmosphere.mieScattering * mieDensity;
    medium.scattering = medium.rayleigh + medium.mie;
    medium.extinction = medium.rayleigh + atmosphere.mieExtinction * mieDensity + atmosphere.ozoneAbsorption * ozoneDensity;
    return medium;
}

float rayleighPhase(float cosTheta)
{
    return 3.0 / (16.0 * PI) * (1.0 + cosTheta * cosTheta);
}

// Cornette-Shanks, Henyey-Greenstein made to fall off as Mie's does.
float miePhase(float g, float cosTheta)
{
    float k = 3.0 / (8.0 * PI) * (1.0 - g * g) / (2.0 + g * g);
    return k * (1.0 + cosTheta * cosTheta) / pow(max(1.0 + g * g - 2.0 * g * cosTheta, 1e-4), 1.5);
}

// The transmittance table's coordinates for a height (from the planet's
// centre) and the cosine of the angle from straight up (Bruneton 2017).
float2 transmittanceUv(Atmosphere atmosphere, float height, float cosZenith)
{
    float top = atmosphere.topRadius;
    float bottom = atmosphere.bottomRadius;
    float h = sqrt(max(0.0, top * top - bottom * bottom));
    float rho = sqrt(max(0.0, height * height - bottom * bottom));
    float discriminant = height * height * (cosZenith * cosZenith - 1.0) + top * top;
    float d = max(0.0, -height * cosZenith + sqrt(max(discriminant, 0.0)));
    float dMin = top - height;
    float dMax = rho + h;
    return float2((d - dMin) / max(dMax - dMin, 1e-6), rho / h);
}

void transmittanceParameters(Atmosphere atmosphere, float2 uv, out float height, out float cosZenith)
{
    float top = atmosphere.topRadius;
    float bottom = atmosphere.bottomRadius;
    float h = sqrt(top * top - bottom * bottom);
    float rho = h * uv.y;
    height = sqrt(rho * rho + bottom * bottom);
    float dMin = top - height;
    float dMax = rho + h;
    float d = dMin + uv.x * (dMax - dMin);
    cosZenith = d == 0.0 ? 1.0 : (h * h - rho * rho - d * d) / (2.0 * height * d);
    cosZenith = clamp(cosZenith, -1.0, 1.0);
}

float3 transmittanceToSpace(Lighting lighting, float height, float cosZenith)
{
    float2 uv = transmittanceUv(lighting.atmosphere, height, cosZenith);
    return textures[lighting.transmittance].SampleLevel(samplers[SAMPLER_LINEAR_CLAMP], uv, 0).rgb;
}

// Light scattered more than once reaching a height with the sun at an angle,
// for the sun's light of one.
float3 multiScatteringAt(Lighting lighting, float height, float cosSunZenith)
{
    Atmosphere atmosphere = lighting.atmosphere;
    float2 uv = float2(cosSunZenith * 0.5 + 0.5, (height - atmosphere.bottomRadius) / (atmosphere.topRadius - atmosphere.bottomRadius));
    uv = fromUnitToSubUvs(saturate(uv), MULTI_SCATTERING_SIZE);
    return textures[lighting.multiScattering].SampleLevel(samplers[SAMPLER_LINEAR_CLAMP], uv, 0).rgb;
}

struct Scattered
{
    // Light scattered towards the ray's origin, for the sun's light of one.
    float3 luminance;
    float3 transmittance;
    // What the medium scatters of light of one from every direction: the
    // share of each scattering event that scatters again, in the table of
    // multiple scattering.
    float3 multiScatteringShare;
};

// Marches a ray through the atmosphere lit by a sun in the direction `sun`,
// out to `farthest` kilometres or the ground or space. `isotropic` scatters
// evenly, as the multiple scattering table does; `multiple` adds what that
// table holds.
Scattered integrate(
    Lighting lighting,
    float3 sun,
    float3 origin,
    float3 direction,
    float farthest,
    uint steps,
    bool ground,
    bool isotropic,
    bool multiple)
{
    Atmosphere atmosphere = lighting.atmosphere;
    Scattered result;
    result.luminance = 0.0;
    result.transmittance = 1.0;
    result.multiScatteringShare = 0.0;

    float toGround = raySphere(origin, direction, atmosphere.bottomRadius);
    float toSpace = raySphere(origin, direction, atmosphere.topRadius);
    float end;
    if (toGround < 0.0)
    {
        if (toSpace < 0.0)
        {
            return result;
        }
        end = toSpace;
    }
    else
    {
        end = toSpace > 0.0 ? min(toSpace, toGround) : toGround;
    }
    bool hitsGround = toGround >= 0.0 && end == toGround && end <= farthest;
    end = min(end, farthest);

    float cosTheta = dot(sun, direction);
    float rayleigh = rayleighPhase(cosTheta);
    float mie = miePhase(atmosphere.mieG, cosTheta);
    float t = 0.0;
    for (uint i = 0; i < steps; i++)
    {
        float next = end * (float(i) + 0.3) / float(steps);
        float dt = next - t;
        t = next;
        float3 position = origin + direction * t;
        Medium medium = mediumAt(atmosphere, position);
        float3 extinction = max(medium.extinction, 1e-6);
        float3 stepTransmittance = exp(-extinction * dt);

        float height = length(position);
        float3 up = position / height;
        float cosSun = dot(sun, up);
        float3 sunTransmittance = transmittanceToSpace(lighting, height, cosSun);
        float shadow = raySphere(position + up * PLANET_OFFSET, sun, atmosphere.bottomRadius) >= 0.0 ? 0.0 : 1.0;
        float3 phased = isotropic ? medium.scattering / (4.0 * PI) : medium.rayleigh * rayleigh + medium.mie * mie;
        float3 multiplied = multiple ? multiScatteringAt(lighting, height, cosSun) * medium.scattering : 0.0;
        float3 scattered = shadow * sunTransmittance * phased + multiplied;

        // Integrated over the step analytically, as the light dims along it.
        result.multiScatteringShare += result.transmittance * (medium.scattering - medium.scattering * stepTransmittance) / extinction;
        result.luminance += result.transmittance * (scattered - scattered * stepTransmittance) / extinction;
        result.transmittance *= stepTransmittance;
    }

    if (ground && hitsGround)
    {
        float3 position = origin + direction * end;
        float height = length(position);
        float3 up = position / height;
        float cosSun = dot(sun, up);
        float3 sunTransmittance = transmittanceToSpace(lighting, height, cosSun);
        result.luminance += sunTransmittance * result.transmittance * saturate(cosSun) * atmosphere.groundAlbedo / PI;
    }
    return result;
}

// The sky-view table's coordinates for a direction: the cosine of its angle
// from straight up, and of its angle round from the sun, with rows packed
// close round the horizon.
float2 skyViewUv(Atmosphere atmosphere, float height, float cosZenith, float cosSunAround, bool hitsGround)
{
    float horizonDistance = sqrt(max(height * height - atmosphere.bottomRadius * atmosphere.bottomRadius, 0.0));
    float beta = acos(clamp(horizonDistance / height, -1.0, 1.0));
    float zenithToHorizon = PI - beta;
    float2 uv;
    if (!hitsGround)
    {
        float coord = acos(clamp(cosZenith, -1.0, 1.0)) / zenithToHorizon;
        uv.y = (1.0 - sqrt(max(1.0 - coord, 0.0))) * 0.5;
    }
    else
    {
        float coord = (acos(clamp(cosZenith, -1.0, 1.0)) - zenithToHorizon) / beta;
        uv.y = sqrt(max(coord, 0.0)) * 0.5 + 0.5;
    }
    uv.x = sqrt(saturate(-cosSunAround * 0.5 + 0.5));
    return fromUnitToSubUvs(uv, SKY_VIEW_SIZE);
}

void skyViewParameters(Atmosphere atmosphere, float height, float2 uv, out float cosZenith, out float cosSunAround)
{
    uv = fromSubUvsToUnit(uv, SKY_VIEW_SIZE);
    float horizonDistance = sqrt(max(height * height - atmosphere.bottomRadius * atmosphere.bottomRadius, 0.0));
    float beta = acos(clamp(horizonDistance / height, -1.0, 1.0));
    float zenithToHorizon = PI - beta;
    if (uv.y < 0.5)
    {
        float coord = 1.0 - 2.0 * uv.y;
        coord = 1.0 - coord * coord;
        cosZenith = cos(zenithToHorizon * coord);
    }
    else
    {
        float coord = uv.y * 2.0 - 1.0;
        cosZenith = cos(zenithToHorizon + beta * coord * coord);
    }
    float coord = uv.x * uv.x;
    cosSunAround = -(coord * 2.0 - 1.0);
}

// The sky's light from a direction as the camera sees it, without the sun's
// disc, for the sun's light of one.
float3 skyToward(Frame* frame, float3 direction)
{
    Atmosphere atmosphere = frame->lighting.atmosphere;
    float3 origin = cameraInAtmosphere(frame);
    float height = length(origin);
    float3 up = origin / height;
    float cosZenith = dot(direction, up);
    float3 side = cross(up, direction);
    side = dot(side, side) > 1e-8 ? normalize(side) : float3(1.0, 0.0, 0.0);
    float3 forward = normalize(cross(side, up));
    float3 sun = towardsSun(frame);
    float2 sunOnPlane = float2(dot(sun, forward), dot(sun, side));
    float cosSunAround = dot(sunOnPlane, sunOnPlane) > 1e-8 ? normalize(sunOnPlane).x : 1.0;
    bool hitsGround = raySphere(origin, direction, atmosphere.bottomRadius) >= 0.0;
    float2 uv = skyViewUv(atmosphere, height, cosZenith, cosSunAround, hitsGround);
    return textures[frame->lighting.skyView].SampleLevel(samplers[SAMPLER_LINEAR_CLAMP], uv, 0).rgb;
}

// The sun's light where the camera is, after its way through the air,
// worked out from the transmittance table.
float3 sunThroughAtmosphereNow(Frame* frame)
{
    float3 origin = cameraInAtmosphere(frame);
    float height = length(origin);
    return frame->sunColor * transmittanceToSpace(frame->lighting, height, dot(towardsSun(frame), origin / height));
}

// The sky's light from all round, without the ground's: straight up and at
// the horizon all round, worked out from the sky view table.
float3 skyAroundNow(Frame* frame)
{
    float3 zenith = skyToward(frame, float3(0.0, 1.0, 0.0));
    float3 horizon = 0.0;
    for (uint i = 0; i < 4; i++)
    {
        float around = float(i) * 0.5 * PI;
        horizon += skyToward(frame, normalize(float3(cos(around), 0.15, sin(around)))) * 0.25;
    }
    return lerp(horizon, zenith, 0.5) * frame->sunColor;
}

// The sun's light where the camera is, after its way through the air: as
// the frame worked it out once, which every pixel shares.
float3 sunThroughAtmosphere(Frame* frame)
{
    SkyLight* light = buffers()->skyLight;
    return light != nullptr ? light->sun.rgb : sunThroughAtmosphereNow(frame);
}

// The aerial perspective table's depth, in slices, at a distance.
float aerialSliceAt(float kilometres)
{
    return sqrt(max(kilometres, 0.0) / (AERIAL_KM_PER_SLICE * AERIAL_SLICES)) * AERIAL_SLICES - 0.5;
}

// What the air adds between the camera and a point `metres` away, seen at
// `screenUv`, and how much of the point's own light it lets through, for
// the sun's light of one.
float4 aerialPerspectiveAt(Frame* frame, float2 screenUv, float metres)
{
    float kilometres = metres * 0.001 * frame->lighting.atmosphere.aerialScale;
    float slice = aerialSliceAt(kilometres);
    // Nearer than the first slice's middle, the haze fades in from nothing.
    float weight = saturate((slice + 0.5) * 2.0);
    slice = clamp(slice, 0.0, AERIAL_SLICES - 1.0);
    float first = floor(slice);
    float second = min(first + 1.0, AERIAL_SLICES - 1.0);
    float2 inSlice = clamp(screenUv * AERIAL_SLICES, 0.5, AERIAL_SLICES - 0.5);
    float width = AERIAL_SLICES * AERIAL_SLICES;
    Texture2D table = textures[frame->lighting.aerialPerspective];
    SamplerState clamped = samplers[SAMPLER_LINEAR_CLAMP];
    float4 a = table.SampleLevel(clamped, float2((first * AERIAL_SLICES + inSlice.x) / width, inSlice.y / AERIAL_SLICES), 0);
    float4 b = table.SampleLevel(clamped, float2((second * AERIAL_SLICES + inSlice.x) / width, inSlice.y / AERIAL_SLICES), 0);
    float4 haze = lerp(a, b, slice - first) * weight;
    return float4(haze.rgb, 1.0 - haze.a);
}

// The light a surface gets from the sky and the ground round it, facing up
// by `up` (1 straight up, 0 straight down): the sky's light straight up and
// at the horizon all round, and the ground as the sun and sky light it.
float3 atmosphereAmbient(Frame* frame, float up, float3 sunLight)
{
    SkyLight* light = buffers()->skyLight;
    float3 sky = light != nullptr ? light->sky.rgb : skyAroundNow(frame);
    float3 albedo = frame->lighting.atmosphere.groundAlbedo;
    float3 ground = albedo * (sunLight * saturate(towardsSun(frame).y) / PI + sky);
    return lerp(ground, sky, up) * frame->ambient;
}
