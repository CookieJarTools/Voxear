// A physically based sky: the Earth's atmosphere as Hillaire models it in
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
// Everything here is in kilometres, with the Earth's centre below the
// world's origin, so the scene's ground at y = 0 is the Earth's surface.

#pragma once

#include "common.slang"
#include "clouds.slang"

static const float2 TRANSMITTANCE_SIZE = float2(256.0, 64.0);
static const float2 MULTI_SCATTERING_SIZE = float2(32.0, 32.0);
static const float2 SKY_VIEW_SIZE = float2(192.0, 108.0);
static const float AERIAL_SLICES = 32.0;
static const float AERIAL_KM_PER_SLICE = 4.0;
// Points are kept this far above the ground, so rays from them do not hit it at once.
static const float GROUND_OFFSET = 0.01;
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

// The distance along a ray to a sphere round the Earth's centre: the near
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

// Where the camera is relative to the Earth's centre, in kilometres: as high as it is over the level's ground,
// wherever the world's middle has moved to.
float3 cameraInAtmosphere(Frame* frame)
{
    Atmosphere atmosphere = frame->lighting.atmosphere;
    float3 camera = frame->cameraPosition;
    float3 position = float3(camera.x, levelHeight(frame, camera), camera.z) * 0.001;
    position += float3(0.0, atmosphere.bottomRadius, 0.0);
    float height = length(position);
    float lowest = atmosphere.bottomRadius + GROUND_OFFSET;
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

// The transmittance table's coordinates for a height (from the Earth's
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
        float shadow = raySphere(position + up * GROUND_OFFSET, sun, atmosphere.bottomRadius) >= 0.0 ? 0.0 : 1.0;
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
    // The colour the air tends to looking this way, as a long enough stretch of it would show: the last slice's
    // light over how much it hides.
    float4 last = table.SampleLevel(clamped, float2(((AERIAL_SLICES - 1.0) * AERIAL_SLICES + inSlice.x) / width, inSlice.y / AERIAL_SLICES), 0);
    float3 endless = last.rgb / max(last.a, 1e-4);
    float through = 1.0 - haze.a;
    // Past the last slice the air goes on as it was: what it lets through falls off as over the last slice's
    // distance, towards the colour it tends to, rather than stopping at the last slice's haze.
    float lastShare = (AERIAL_SLICES - 0.5) / AERIAL_SLICES;
    float lastKm = lastShare * lastShare * AERIAL_KM_PER_SLICE * AERIAL_SLICES;
    if (kilometres > lastKm && last.a > 1e-4)
    {
        float further = pow(max(1.0 - last.a, 1e-6), (kilometres - lastKm) / lastKm);
        haze.rgb += through * (1.0 - further) * endless;
        through *= further;
    }
    // Within the visibility, the last of the land fades into the sky: no more than 2% of it shows through at that
    // distance.
    float visibility = frame->lighting.atmosphere.visibility;
    if (visibility > 0.0)
    {
        float kept = exp(-3.912 * metres / visibility);
        haze.rgb += through * (1.0 - kept) * endless;
        through *= kept;
    }
    return float4(haze.rgb, through);
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

// ---------------------------------------------------------------- Clouds' light, and what falls from them

// The sun's light on the clouds and the sky's, as the frame has them.
void cloudLight(Frame* frame, out float3 sun, out float3 sky)
{
    if (frame->lighting.sky == SKY_ATMOSPHERE)
    {
        sun = sunThroughAtmosphere(frame);
        SkyLight* light = buffers()->skyLight;
        sky = light != nullptr ? light->sky.rgb : skyAroundNow(frame);
        return;
    }
    sun = frame->sunColor;
    sky = lerp(frame->skyHorizon, frame->skyZenith, 0.5);
}

// `color`, the light of `covered` of what lies `distance` metres off along the ray seen at `screenUv`, hazed by the
// air in front of it as the clouds are. Under the atmosphere the air is counted a third as hazy as the ground's,
// which is far nearer; a gradient sky's horizon takes over from a fifth of the clouds' reach out.
float3 cloudsHazed(Frame* frame, float2 screenUv, float3 color, float covered, float distance)
{
    if (frame->lighting.sky == SKY_ATMOSPHERE)
    {
        float4 haze = aerialPerspectiveAt(frame, screenUv, distance * 0.3);
        return color * haze.a + haze.rgb * skyLook(frame) * covered;
    }
    return lerp(color, frame->skyHorizon * covered, smoothstep(0.2, 1.0, distance / CLOUD_REACH));
}

// The light on the undersides of the clouds over `xz` in the world, seen along `direction`, as the Clouds pass
// lights them (sky.slang's cloudsMarched and cloudBlocksSeen): the sun through the cloud between them and it, the
// light scattered round inside, and the sky's, all of it dimmed under heavy weather.
float3 cloudUndersideLight(Frame* frame, float2 xz, float3 direction, float3 sun, float3 sky)
{
    Clouds clouds = frame->lighting.clouds;
    float3 toSun = towardsSun(frame);
    float heavy = cloudWeather(frame, xz).y;
    if (clouds.kind == CLOUDS_BLOCKS)
    {
        // The lowest cube's underside, and the cubes between its middle and the sun.
        float block = max(clouds.block, 1.0);
        float3 low = float3(xz.x - clouds.drift.x, 0.5 * block, xz.y - clouds.drift.y);
        float shade = 0.0;
        for (uint j = 1; j <= (cloudsOn(frame) ? CLOUD_SUN_STEPS : 0); j++)
        {
            float3 p = low + toSun * (block * 1.2 * float(j));
            shade += cloudCubeAt(frame, p.xz, p.y / clouds.thickness);
        }
        float3 solid = sun * (0.4 - 0.14) * 0.95 + sky * (0.7 - 0.3);
        float3 mist = sun * (0.3 + 0.65 * exp(-0.7 * shade)) * 0.95 + sky * (0.45 + 0.55 * low.y / clouds.thickness);
        return lerp(solid, mist, clouds.softness) * (1.0 - 0.72 * heavy);
    }
    // A little way in from the underside, where a ray from below meets the cloud; the cloud towards the sun in steps
    // that double, as each of the march's steps finds it.
    const float height = 0.08;
    float shade = 0.0;
    float reach = clouds.thickness * 0.04;
    float along = 0.0;
    for (uint j = 0; j < (cloudsOn(frame) ? CLOUD_SUN_STEPS : 0); j++)
    {
        float middle = along + reach * 0.5;
        shade += cloudDensityAt(frame, xz + toSun.xz * middle, height + toSun.y * middle / clouds.thickness, false) * reach;
        along += reach;
        reach *= 2.0;
    }
    float direct = exp(-shade);
    float scattered = 1.0 / (1.0 + 0.35 * shade);
    float lining = min(miePhase(0.75, dot(direction, toSun)) * 4.0 * PI, 6.0);
    float lit = (0.3 * direct + 0.7 * scattered) * 0.95 + direct * lining * 0.15;
    return (sun * lit + sky * (0.3 + 0.7 * height)) * (1.0 - 0.5 * heavy);
}

// What falls from the weather over parts of the level, seen as a curtain hanging under its cloud, uneven, and
// thinner towards the ground; blown sand is a wall a few hundred metres high. Rain and snow are lit as the
// undersides of the clouds they fall from are, by the same sun and sky and as dark, and hazed with distance as those
// clouds are, so a storm's cloud and what falls from it are one grey; blown sand is the colour of sand in the sky's
// light. Gives what all of it adds in front of what lies `distance` metres off along `direction`, seen at
// `screenUv`, and in alpha how much of that shows through it.
static const float SAND_WALL = 350.0;
float4 weatherCurtain(Frame* frame, float3 direction, float distance, float2 screenUv)
{
    float flat = length(direction.xz);
    if (frame->sea.storms == 0 || flat < 1e-4)
    {
        return float4(0.0, 0.0, 0.0, 1.0);
    }
    float2 along = direction.xz / flat;
    float reach = distance * flat;
    Clouds clouds = frame->lighting.clouds;
    // How high the camera is over the level's ground, and how high the clouds' undersides are.
    float camera = levelHeight(frame, frame->cameraPosition);
    float under = clouds.base + 0.15 * clouds.thickness;
    float3 color = 0.0;
    float through = 1.0;
    // The light, worked out for the first fall seen.
    bool lit = false;
    float3 sun = 0.0;
    float3 sky = 0.0;
    float3 sand = 0.0;
    for (uint i = 0; i < frame->sea.storms; i++)
    {
        float4 place = frame->sea.places[i];
        float2 falls = frame->sea.skies[i].zw * float2(1.0, 2.5);
        if (falls.x + falls.y <= 0.0)
        {
            continue;
        }
        float2 to = place.xy - frame->cameraPosition.xz;
        float nearest = dot(to, along);
        float aside = dot(to, to) - nearest * nearest;
        float within = place.z * place.z - aside;
        if (within <= 0.0)
        {
            continue;
        }
        float half = sqrt(within);
        float from = max(nearest - half, 0.0);
        float until = min(nearest + half, reach);
        if (until <= from)
        {
            continue;
        }
        const uint SAMPLES = 6;
        float2 thick = 0.0;
        // How far along the ground the fall is on the whole, by how much of it is where.
        float2 middle = 0.0;
        for (uint j = 0; j < SAMPLES; j++)
        {
            float t = lerp(from, until, (float(j) + 0.5) / float(SAMPLES));
            float2 xz = frame->cameraPosition.xz + along * t;
            float high = camera + direction.y * t / flat;
            // Heavier here and lighter there, in shafts.
            float shafts = 0.45 + 1.1 * cloudPatches(levelXZ(frame, xz, 65536.0) / max(place.z * 0.22, 40.0));
            float share = weatherShare(frame, i, xz) * shafts;
            // Rain and snow from the clouds' undersides down, thinning as they fall; sand from the ground up.
            float rain = saturate((under - high) / (0.1 * clouds.thickness)) * (0.5 + 0.5 * saturate(high / under));
            float wall = saturate((camera + SAND_WALL * (0.7 + 0.6 * shafts) - high) / 60.0);
            float2 here = share * float2(rain, wall) * falls;
            thick += here;
            middle += float2(t, 1.0) * (here.x + here.y);
        }
        // A heavy fall hides what is a few hundred metres into it.
        thick *= (until - from) / float(SAMPLES) / flat * 0.004;
        float kept = exp(-thick.x - thick.y);
        float added = through * (1.0 - kept);
        through *= kept;
        if (added <= 1e-4)
        {
            continue;
        }
        if (!lit)
        {
            cloudLight(frame, sun, sky);
            float3 level = normalize(float3(direction.x, 0.02, direction.z));
            float3 horizon = frame->lighting.sky == SKY_ATMOSPHERE ? skyToward(frame, level) * skyLook(frame) : frame->skyHorizon;
            sand = float3(0.56, 0.4, 0.22) * dot(horizon, float3(0.2126, 0.7152, 0.0722)) * 1.7;
            lit = true;
        }
        float t = middle.x / max(middle.y, 1e-6);
        float3 falling = cloudUndersideLight(frame, frame->cameraPosition.xz + along * t, direction, sun, sky);
        float3 seen = lerp(falling, sand, thick.y / max(thick.x + thick.y, 1e-6));
        color += cloudsHazed(frame, screenUv, seen, 1.0, t / flat) * added;
    }
    return float4(color, through);
}
