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
    Clouds clouds = frame->lighting.clouds;
    if (clouds.cover > 0.0 && clouds.view != NO_TEXTURE)
    {
        // The clouds in front of all of it: what they add, and how much of the sky behind shows through.
        float4 seen = textures[NonUniformResourceIndex(clouds.view)].SampleLevel(samplers[SAMPLER_LINEAR_CLAMP], input.ndc * 0.5 + 0.5, 0.0);
        color = color * seen.a + seen.rgb;
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
    float height = atmosphere.bottomRadius + saturate(uv.y) * (atmosphere.topRadius - atmosphere.bottomRadius - GROUND_OFFSET)
        + GROUND_OFFSET;
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

// The sun's light where the camera is and the sky's from all round, for this
// frame, worked out once from the tables for every pixel to share.
[shader("compute")]
[numthreads(1, 1, 1)]
void skyLightMain()
{
    Frame* frame = push.frame;
    SkyLight* light = buffers()->skyLight;
    light->sun = float4(sunThroughAtmosphereNow(frame), 0.0);
    // Under clouds the sky's light is greyer, the more of it they cover: as bright, less blue.
    float3 sky = skyAroundNow(frame);
    float cover = frame->lighting.clouds.cover;
    float grey = dot(sky, float3(0.2126, 0.7152, 0.0722));
    light->sky = float4(lerp(sky, grey, cover * 0.7), 0.0);
}

// ---------------------------------------------------------------- Clouds

// How far the clouds are marched from the camera, in metres, fading out over the last third; how many steps through
// them, and towards the sun from each.
static const float CLOUD_REACH = 36000.0;
static const uint CLOUD_STEPS = 40;
static const uint CLOUD_SUN_STEPS = 4;
// How much of last frame's picture this frame's keeps.
static const float CLOUD_KEPT = 0.9;
// The Earth's radius, in metres.
static const float CLOUD_EARTH = 6360000.0;

// The two distances along a ray from a camera `camera` metres over the ground, climbing `climb` a metre, to
// `height` over the ground round the Earth; false where it never reaches it. Worked out from the heights' difference,
// which stays exact where the radii's squares would not.
bool cloudShell(float camera, float climb, float height, out float near, out float far)
{
    float b = (CLOUD_EARTH + camera) * climb;
    float c = (camera - height) * (2.0 * CLOUD_EARTH + camera + height);
    float discriminant = b * b - c;
    near = 0.0;
    far = 0.0;
    if (discriminant < 0.0)
    {
        return false;
    }
    float root = sqrt(discriminant);
    if (b >= 0.0)
    {
        near = -(b + root);
        far = near != 0.0 ? c / near : 0.0;
    }
    else
    {
        far = root - b;
        far = max(far, 1e-6);
        near = c / far;
    }
    return true;
}

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

// The sun's light let through the clouds to the middle of their layer over each place round the camera, looking
// down: red alone is read. Each texel's samples lie along the way to the sun, so a shadow falls where that way
// meets the ground, not straight under its cloud.
[shader("fragment")]
float4 cloudShadowsFragment(LutOut input) : SV_Target0
{
    Frame* frame = push.frame;
    Clouds clouds = frame->lighting.clouds;
    float2 middle = clouds.shadowMiddle + (input.uv - 0.5) * 2.0 * clouds.shadowReach;
    float3 toSun = towardsSun(frame);
    float2 slant = toSun.xz / max(toSun.y, 0.12) * clouds.thickness;
    const uint SAMPLES = 6;
    float through = 0.0;
    for (uint i = 0; i < SAMPLES; i++)
    {
        float height = (float(i) + 0.5) / float(SAMPLES);
        through += cloudDensityAt(frame, middle + slant * (height - 0.5), height, false);
    }
    // A slanting sun has further to go through them.
    through *= clouds.thickness / float(SAMPLES) / max(toSun.y, 0.3);
    return float4(exp(-through), 0.0, 0.0, 1.0);
}

// Soft clouds along a ray from a camera `camera` metres over the ground: what they add in colour, in alpha how much
// of the sky behind shows through, and in `distance` how far off they are, in metres. The layer is marched from a
// start that moves each frame.
float4 cloudsMarched(Frame* frame, float2 pixel, float3 direction, float camera, float3 sun, float3 sky, out float distance)
{
    Clouds clouds = frame->lighting.clouds;
    float top = clouds.base + clouds.thickness;
    distance = 0.0;

    // The stretch of the ray inside the layer: within its tops' height, outside its undersides'.
    float4 nothing = float4(0.0, 0.0, 0.0, 1.0);
    float topNear, topFar, baseNear, baseFar, groundNear, groundFar;
    if (!cloudShell(camera, direction.y, top, topNear, topFar) || topFar <= 0.0)
    {
        return nothing;
    }
    float from = max(topNear, 0.0);
    float to = topFar;
    if (cloudShell(camera, direction.y, clouds.base, baseNear, baseFar))
    {
        if (baseNear > from)
        {
            to = min(to, baseNear);
        }
        else
        {
            from = max(from, baseFar);
        }
    }
    // Not through the ground.
    if (cloudShell(camera, direction.y, 0.0, groundNear, groundFar) && groundNear > 0.0 && groundNear < from)
    {
        return nothing;
    }
    to = min(to, CLOUD_REACH);
    if (to <= from)
    {
        return nothing;
    }
    // At a low angle the way through is long: the steps stay short enough to find a cloud in.
    to = min(to, from + clouds.thickness * 12.0);

    float3 toSun = towardsSun(frame);
    float toward = dot(direction, toSun);
    // Brightest looking towards the sun through a thin edge, the silver lining.
    float lining = min(miePhase(0.75, toward) * 4.0 * PI, 6.0);

    float jitter = frac(52.9829189 * frac(dot(pixel + 5.588238 * float(frame->frameIndex % 64), float2(0.06711056, 0.00583715))));
    // Another for the steps towards the sun, so the light's edge inside a cloud is not a line.
    float sunJitter = frac(jitter * 7.31 + 0.37);
    float step = (to - from) / float(CLOUD_STEPS);
    float flat = 1.0 - direction.y * direction.y;
    float3 color = 0.0;
    float through = 1.0;
    float weight = 0.0;
    for (uint i = 0; i < CLOUD_STEPS; i++)
    {
        float t = from + (float(i) + jitter) * step;
        // How high the ray is here, over ground that curves away under it.
        float up = camera + direction.y * t + t * t * flat / (2.0 * CLOUD_EARTH);
        float height = (up - clouds.base) / clouds.thickness;
        float2 xz = frame->cameraPosition.xz + direction.xz * t;
        float density = cloudDensityAt(frame, xz, height, true);
        if (density <= 0.0)
        {
            continue;
        }
        // How much cloud lies between here and the sun: steps that double, so the near ones find the cloud's own
        // edge and the far ones the clouds over it.
        float shade = 0.0;
        float reach = clouds.thickness * 0.04;
        float along = 0.0;
        for (uint j = 0; j < CLOUD_SUN_STEPS; j++)
        {
            float middle = along + reach * (0.25 + 0.5 * sunJitter);
            shade += cloudDensityAt(frame, xz + toSun.xz * middle, height + toSun.y * middle / clouds.thickness, false) * reach;
            along += reach;
            reach *= 2.0;
        }
        // The sun straight through, and the light scattered round inside the cloud, which reaches far deeper.
        float direct = exp(-shade);
        float scattered = exp(-shade * 0.2);
        float lit = (0.3 * direct + 0.7 * scattered) * 0.95 + direct * lining * 0.15;
        // The sky lights their tops more than their undersides, which get the ground's.
        float3 light = sun * lit + sky * (0.3 + 0.7 * saturate(height));
        float kept = exp(-density * step);
        float added = through * (1.0 - kept);
        color += light * added;
        weight += added;
        distance += t * added;
        through *= kept;
        if (through < 0.01)
        {
            through = 0.0;
            break;
        }
    }
    if (weight <= 0.0)
    {
        return nothing;
    }
    distance /= weight;
    return float4(color, through);
}

// How far clouds built of cubes are drawn, in metres, fading out over the last third, and how many cubes a ray is
// walked through at most.
static const float CLOUD_BLOCK_REACH = 16000.0;
static const uint CLOUD_BLOCK_STEPS = 128;

// Clouds built of cubes along a ray, as cloudsMarched gives soft ones: the cubes are walked one at a time to the
// first that is cloud, and its face lit all one colour, by where it faces and by the cubes between it and the sun.
// Their layer is flat: they are not drawn far enough off for the Earth's curve to show.
float4 cloudBlocksSeen(Frame* frame, float3 direction, float camera, float3 sun, float3 sky, out float distance)
{
    Clouds clouds = frame->lighting.clouds;
    float4 nothing = float4(0.0, 0.0, 0.0, 1.0);
    distance = 0.0;
    float block = max(clouds.block, 1.0);
    // The stretch of the ray between the layer's underside and its top.
    float low = clouds.base - camera;
    float high = low + clouds.thickness;
    float from = 0.0;
    float to = CLOUD_BLOCK_REACH;
    if (abs(direction.y) > 1e-5)
    {
        float a = low / direction.y;
        float b = high / direction.y;
        from = max(min(a, b), 0.0);
        to = min(max(a, b), to);
    }
    else if (low > 0.0 || high < 0.0)
    {
        return nothing;
    }
    if (to <= from)
    {
        return nothing;
    }
    // In cubes: across the clouds' own pattern, and up from the layer's underside.
    float3 origin = float3(frame->cameraPosition.x - clouds.drift.x, -low, frame->cameraPosition.z - clouds.drift.y) / block;
    float3 rate = direction / block;
    float3 toward = float3(direction.x >= 0.0 ? 1.0 : -1.0, direction.y >= 0.0 ? 1.0 : -1.0, direction.z >= 0.0 ? 1.0 : -1.0);
    float3 cell = floor(origin + rate * (from + 0.001 * block));
    // How far along the ray each axis's next wall is, and how far apart its walls are.
    float3 span = 1.0 / max(abs(rate), 1e-9);
    float3 next = (cell + max(toward, 0.0) - origin) * toward * span;
    // The face the ray comes in by: the layer's own, or none for a camera inside it.
    float3 normal = from > 0.0 ? float3(0.0, -toward.y, 0.0) : float3(0.0);
    float t = from;
    bool found = false;
    for (uint i = 0; i < CLOUD_BLOCK_STEPS; i++)
    {
        float3 middle = (cell + 0.5) * block;
        if (cloudDensityIn(frame, middle.xz, middle.y / clouds.thickness, false) > 0.0)
        {
            found = true;
            break;
        }
        if (next.x < next.y && next.x < next.z)
        {
            t = next.x;
            next.x += span.x;
            cell.x += toward.x;
            normal = float3(-toward.x, 0.0, 0.0);
        }
        else if (next.y < next.z)
        {
            t = next.y;
            next.y += span.y;
            cell.y += toward.y;
            normal = float3(0.0, -toward.y, 0.0);
        }
        else
        {
            t = next.z;
            next.z += span.z;
            cell.z += toward.z;
            normal = float3(0.0, 0.0, -toward.z);
        }
        if (t >= to)
        {
            break;
        }
    }
    if (!found)
    {
        return nothing;
    }
    // The cubes between the face and the sun, from the open cube it looks into.
    float3 toSun = towardsSun(frame);
    float3 open = (cell + 0.5 + normal) * block;
    float shade = 0.0;
    for (uint j = 1; j <= CLOUD_SUN_STEPS; j++)
    {
        float3 p = open + toSun * (block * 1.2 * float(j));
        shade += cloudDensityIn(frame, p.xz, p.y / clouds.thickness, false) > 0.0 ? 1.0 : 0.0;
    }
    float facing = saturate(dot(normal, toSun)) * lerp(1.0, 0.3, saturate(shade * 0.5));
    // Some of the sun gets through a cloud to every face, most to its top and least to its underside, and the sky
    // lights them in the same order: so each way a face can look has a shade of its own.
    float3 light = sun * (0.4 + 0.14 * normal.y + 0.5 * facing) * 0.95 + sky * (0.7 + 0.3 * normal.y);
    distance = t;
    return float4(light, 0.0);
}

// The clouds as the camera sees them, at half the picture's size: what they add in colour, and in alpha how much of
// the sky behind them shows through. The result is blended with last frame's where the same part of the sky was then
// (push.textures.x), which evens out the soft clouds' jittered march and smooths the cubes' edges.
[shader("fragment")]
float4 cloudsFragment(LutOut input) : SV_Target0
{
    Frame* frame = push.frame;
    Clouds clouds = frame->lighting.clouds;
    float4 near = transform(frame->invViewProj, float4(input.uv * 2.0 - 1.0, 1.0, 1.0));
    float3 direction = normalize(near.xyz / near.w - frame->cameraPosition);
    float camera = max(levelHeight(frame, frame->cameraPosition), 1.0);
    float3 sun, sky;
    cloudLight(frame, sun, sky);

    bool blocks = clouds.kind == CLOUDS_BLOCKS;
    float4 seen;
    float distance;
    float reach = blocks ? CLOUD_BLOCK_REACH : CLOUD_REACH;
    if (blocks)
    {
        // A place within the pixel that moves each frame, so the frames together smooth the cubes' edges.
        float index = float(frame->frameIndex % 16) + 1.0;
        float2 within = frac(index * float2(0.7548776662, 0.5698402910)) - 0.5;
        float2 uv = input.uv + within * float2(ddx(input.uv.x), ddy(input.uv.y));
        float4 through = transform(frame->invViewProj, float4(uv * 2.0 - 1.0, 1.0, 1.0));
        seen = cloudBlocksSeen(frame, normalize(through.xyz / through.w - frame->cameraPosition), camera, sun, sky, distance);
    }
    else
    {
        seen = cloudsMarched(frame, input.position.xy, direction, camera, sun, sky, distance);
    }
    float covered = 1.0 - seen.a;
    if (covered > 0.0)
    {
        float3 color = seen.rgb;
        if (frame->lighting.sky == SKY_ATMOSPHERE)
        {
            // The air between: counted a third as hazy as the ground's, which is far nearer.
            float4 haze = aerialPerspectiveAt(frame, input.uv, distance * 0.3);
            color = color * haze.a + haze.rgb * skyLook(frame) * covered;
        }
        else
        {
            color = lerp(color, frame->skyHorizon * covered, smoothstep(0.2, 1.0, distance / CLOUD_REACH));
        }
        // Gone into the distance before the march's end.
        float far = 1.0 - smoothstep(0.65, 1.0, distance / reach);
        seen = float4(color * far, 1.0 - covered * far);
    }
    if (clouds.reset != 0)
    {
        return seen;
    }
    // Last frame's, where this part of the sky was on screen then: the clouds are far off, so only the camera's
    // turning moves them.
    float4 before = transform(frame->prevViewProj, float4(direction, 0.0));
    if (before.w <= 0.0)
    {
        return seen;
    }
    float2 was = before.xy / before.w * 0.5 + 0.5;
    if (any(was < 0.0) || any(was > 1.0))
    {
        return seen;
    }
    float4 kept = textures[NonUniformResourceIndex(push.textures.x)].SampleLevel(samplers[SAMPLER_LINEAR_CLAMP], was, 0.0);
    return lerp(seen, kept, CLOUD_KEPT);
}
