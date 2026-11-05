// Local lights as the forward pass shades them from a froxel's list: point
// and spot lights, rectangle and disc area lights by linearly transformed
// cosines (Heitz et al. 2016), and tube lights by Karis's representative
// point (2013). Point and spot lights can carry an IES profile, and spot
// lights a cookie.
//
// Every light's range windows it smoothly to nothing, measured from the
// nearest point of its shape, so a light's reach is the same for every
// shape. An area light's intensity is what a point light of the same
// intensity gives straight ahead of it from far away: its radiance is the
// intensity over its area.

#pragma once

#include "common.slang"
#include "custom.slang"

static const float LTC_SIZE = 64.0;
// Corners of the polygon a disc is drawn as, its radius stretched so the
// polygon's area matches the disc's.
static const uint DISC_CORNERS = 16;
static const uint MAX_CORNERS = 16;
// Nearer than this, a tube's ends count as this far, so nothing divides by 0.
static const float MIN_DISTANCE = 1e-4;

// GGX's specular alone, as `shade` gives it, for lights whose diffuse is
// worked out apart from it.
float3 specularGgx(Surface s, float3 toLight, float3 radiance)
{
    float nl = dot(s.normal, toLight);
    if (nl <= 0.0)
    {
        return 0.0;
    }
    float3 h = normalize(s.view + toLight);
    float nv = max(dot(s.normal, s.view), 1e-4);
    float a = s.roughness * s.roughness;
    float3 f = fresnelSchlick(s.f0, saturate(dot(s.view, h)));
    return distributionGgx(saturate(dot(s.normal, h)), a) * visibilitySmithGgx(nv, nl, a) * f * radiance * nl;
}

// Inverse square's smooth window: 1 close by, falling to 0 at the range.
float rangeWindow(float distanceSquared, float range)
{
    float ratio = distanceSquared / max(range * range, 1e-4);
    float window = saturate(1.0 - ratio * ratio);
    return window * window;
}

float3 lightUp(Light light)
{
    return cross(light.direction, light.right);
}

// An IES profile's share of its peak in the direction `out` leaves the
// light: down the texture from its axis (0) to straight back (1), and across
// it round the axis from the light's +X towards its +Y.
float profileAt(Light light, float3 out)
{
    float vertical = acos(clamp(dot(out, light.direction), -1.0, 1.0)) / PI;
    float horizontal = atan2(dot(out, lightUp(light)), dot(out, light.right)) / (2.0 * PI);
    // Rows at the poles are not blended with the opposite pole by the repeating sampler.
    float2 uv = float2(horizontal, clamp(vertical, 0.5 / 64.0, 1.0 - 0.5 / 64.0));
    return textures[NonUniformResourceIndex(light.profile)].SampleLevel(samplers[SAMPLER_LINEAR_REPEAT], uv, 0).r;
}

// A spot light's cookie where `out` leaves it, stretched over the square
// round its outer cone; outside it, nothing.
float3 cookieAt(Light light, float3 out)
{
    float3 local = float3(dot(out, light.right), dot(out, lightUp(light)), dot(out, light.direction));
    if (local.z <= 1e-4)
    {
        return 0.0;
    }
    float tanOuter = sqrt(max(1.0 - light.cosOuter * light.cosOuter, 1e-6)) / max(light.cosOuter, 1e-4);
    float2 uv = float2(local.x, -local.y) / (local.z * tanOuter) * 0.5 + 0.5;
    if (any(uv < 0.0) || any(uv > 1.0))
    {
        return 0.0;
    }
    return textures[NonUniformResourceIndex(light.cookie)].SampleLevel(samplers[SAMPLER_LINEAR_CLAMP], uv, 0).rgb;
}

float3 shadePunctualLight(Surface s, Light light, float3 position)
{
    float3 toLight = light.position - position;
    float distanceSquared = max(dot(toLight, toLight), 1e-4);
    toLight *= rsqrt(distanceSquared);
    float attenuation = rangeWindow(distanceSquared, light.range) / distanceSquared;
    float3 color = light.color;
    if (light.kind == LIGHT_SPOT)
    {
        attenuation *= smoothstep(light.cosOuter, light.cosInner, dot(-toLight, light.direction));
        if (light.cookie != NO_TEXTURE)
        {
            color *= cookieAt(light, -toLight);
        }
    }
    if (light.profile != NO_TEXTURE)
    {
        attenuation *= profileAt(light, -toLight);
    }
    return shade(s, toLight, color * attenuation);
}

// The integral of a clamped cosine over the arc from `a` to `b` (unit
// vectors), as a vector: its sum over a polygon's edges is the polygon's
// vector form factor. A fit of θ/sin θ / 2π that holds at every angle
// (Hill and Heitz 2017).
float3 edgeIntegral(float3 a, float3 b)
{
    float x = dot(a, b);
    float y = abs(x);
    float numerator = 0.8543985 + (0.4965155 + 0.0145206 * y) * y;
    float denominator = 3.4175940 + (4.1616724 + y) * y;
    float v = numerator / denominator;
    float thetaSinTheta = x > 0.0 ? v : 0.5 * rsqrt(max(1.0 - x * x, 1e-7)) - v;
    return cross(a, b) * thetaSinTheta;
}

// The form factor of the sphere whose vector form factor is `f`, cut by the
// horizon (Lagarde and de Rousiers 2014, as Heitz's sphere lookup tabulates):
// it stands in for clipping the polygon itself.
float horizonClipped(float3 f)
{
    float sinSigmaSquared = min(length(f), 0.9999);
    if (sinSigmaSquared <= 1e-7)
    {
        return 0.0;
    }
    float cosTheta = clamp(f.z / sinSigmaSquared, -1.0, 1.0);
    if (cosTheta * cosTheta > sinSigmaSquared)
    {
        return sinSigmaSquared * saturate(cosTheta);
    }
    float sinTheta = sqrt(max(1.0 - cosTheta * cosTheta, 1e-7));
    float x = sqrt(1.0 / sinSigmaSquared - 1.0);
    float y = -x * cosTheta / sinTheta;
    float sinThetaSqrtY = sinTheta * sqrt(max(1.0 - y * y, 0.0));
    float illuminance = (cosTheta * acos(clamp(y, -1.0, 1.0)) - x * sinThetaSqrtY) * sinSigmaSquared + atan(sinThetaSqrtY / x);
    return max(illuminance, 0.0) / PI;
}

// An LTC's inverse matrix, as the table holds it, applied to a vector in
// the frame of the surface (its view tangent, bitangent and normal).
float3 applyLtc(float4 inverse, float3 v)
{
    return float3(inverse.x * v.x + inverse.z * v.z, v.y, inverse.y * v.x + inverse.w * v.z);
}

// The integral of an LTC over a convex polygon round the point being shaded,
// its corners relative to the point in the surface's frame.
float integratePolygon(float4 inverse, float3 corners[MAX_CORNERS], uint count, float3 centre)
{
    float3 first = normalize(applyLtc(inverse, corners[0]));
    float3 previous = first;
    float3 sum = 0.0;
    for (uint i = 1; i < count; i++)
    {
        float3 next = normalize(applyLtc(inverse, corners[i]));
        sum += edgeIntegral(previous, next);
        previous = next;
    }
    sum += edgeIntegral(previous, first);
    // Whichever way its corners wind, the form factor points at the polygon.
    if (dot(sum, applyLtc(inverse, centre)) < 0.0)
    {
        sum = -sum;
    }
    return horizonClipped(sum);
}

// A rectangle or disc light's diffuse and specular, from its corners round
// the point being shaded, in world space.
float3 shadePolygon(Surface s, Light light, float3 corners[MAX_CORNERS], uint count, float area, float3 position)
{
    Frame* frame = push.frame;
    float nv = saturate(dot(s.normal, s.view));
    float2 uv = float2(s.roughness, sqrt(1.0 - nv)) * ((LTC_SIZE - 1.0) / LTC_SIZE) + 0.5 / LTC_SIZE;
    SamplerState clamped = samplers[SAMPLER_LINEAR_CLAMP];
    float4 inverse = textures[frame->lighting.ltcMatrix].SampleLevel(clamped, uv, 0);
    float2 amplitude = textures[frame->lighting.ltcAmplitude].SampleLevel(clamped, uv, 0).xy;

    // The surface's frame, with the view in its XZ plane as the tables were fitted.
    float3 tangent = s.view - s.normal * dot(s.view, s.normal);
    tangent = dot(tangent, tangent) > 1e-8 ? normalize(tangent)
        : normalize(cross(s.normal, abs(s.normal.y) < 0.99 ? float3(0.0, 1.0, 0.0) : float3(1.0, 0.0, 0.0)));
    float3 bitangent = cross(s.normal, tangent);
    float3 local[MAX_CORNERS];
    for (uint i = 0; i < count; i++)
    {
        float3 v = corners[i] - position;
        local[i] = float3(dot(v, tangent), dot(v, bitangent), dot(v, s.normal));
    }
    float3 toCentre = light.position - position;
    float3 centre = float3(dot(toCentre, tangent), dot(toCentre, bitangent), dot(toCentre, s.normal));

    float diffuse = integratePolygon(float4(1.0, 0.0, 0.0, 1.0), local, count, centre);
    float specular = integratePolygon(inverse, local, count, centre);
    float3 fresnel = s.f0 * amplitude.x + (1.0 - s.f0) * amplitude.y;
    float3 radiance = light.color / max(area, 1e-6);
    return radiance * (s.diffuse * diffuse + fresnel * specular);
}

float3 shadeRectangleLight(Surface s, Light light, float3 position)
{
    // Only the side it faces is lit.
    float3 fromLight = position - light.position;
    if (dot(fromLight, light.direction) <= 0.0)
    {
        return 0.0;
    }
    float3 up = lightUp(light);
    float2 halfSize = light.size;
    float2 onPlane = clamp(float2(dot(fromLight, light.right), dot(fromLight, up)), -halfSize, halfSize);
    float3 nearest = light.position + light.right * onPlane.x + up * onPlane.y;
    float3 offset = position - nearest;
    float window = rangeWindow(dot(offset, offset), light.range);
    if (window <= 0.0)
    {
        return 0.0;
    }
    float3 x = light.right * halfSize.x;
    float3 y = up * halfSize.y;
    float3 corners[MAX_CORNERS];
    corners[0] = light.position - x - y;
    corners[1] = light.position + x - y;
    corners[2] = light.position + x + y;
    corners[3] = light.position - x + y;
    return shadePolygon(s, light, corners, 4, 4.0 * halfSize.x * halfSize.y, position) * window;
}

float3 shadeDiscLight(Surface s, Light light, float3 position)
{
    float3 fromLight = position - light.position;
    if (dot(fromLight, light.direction) <= 0.0)
    {
        return 0.0;
    }
    float radius = light.size.x;
    float3 up = lightUp(light);
    float2 onPlane = float2(dot(fromLight, light.right), dot(fromLight, up));
    float along = length(onPlane);
    onPlane *= along > radius ? radius / along : 1.0;
    float3 offset = position - (light.position + light.right * onPlane.x + up * onPlane.y);
    float window = rangeWindow(dot(offset, offset), light.range);
    if (window <= 0.0)
    {
        return 0.0;
    }
    // A regular polygon of the disc's area.
    float step = 2.0 * PI / float(DISC_CORNERS);
    float stretched = radius * sqrt(step / sin(step));
    float3 corners[MAX_CORNERS];
    for (uint i = 0; i < DISC_CORNERS; i++)
    {
        float angle = float(i) * step;
        corners[i] = light.position + (light.right * cos(angle) + up * sin(angle)) * stretched;
    }
    return shadePolygon(s, light, corners, DISC_CORNERS, PI * radius * radius, position) * window;
}

// A tube: its diffuse as a line's, the cosine averaged over its ends; its
// specular from the point of it nearest the reflected ray, then of the
// sphere of its radius there, with the lobe widened to keep its energy.
float3 shadeTubeLight(Surface s, Light light, float3 position)
{
    float halfLength = light.size.x;
    float radius = light.size.y;
    float3 l0 = light.position - light.right * halfLength - position;
    float3 l1 = light.position + light.right * halfLength - position;
    float3 along = l1 - l0;
    float t = saturate(dot(-l0, along) / max(dot(along, along), 1e-8));
    float3 nearest = l0 + along * t;
    float nearestDistance = max(length(nearest) - radius, 0.0);
    float window = rangeWindow(nearestDistance * nearestDistance, light.range);
    if (window <= 0.0)
    {
        return 0.0;
    }

    float length0 = max(length(l0), MIN_DISTANCE);
    float length1 = max(length(l1), MIN_DISTANCE);
    float nl = saturate(0.5 * (dot(s.normal, l0) / length0 + dot(s.normal, l1) / length1));
    float lineFalloff = 2.0 / max(length0 * length1 + dot(l0, l1), 1e-4);
    float3 diffuse = s.diffuse / PI * light.color * nl * lineFalloff;

    float3 r = reflect(-s.view, s.normal);
    float rAlong = dot(r, along);
    float rt = (dot(r, l0) * rAlong - dot(l0, along)) / max(dot(along, along) - rAlong * rAlong, 1e-6);
    float3 l = l0 + along * saturate(rt);
    float3 centreToRay = dot(l, r) * r - l;
    l += centreToRay * saturate(radius / max(length(centreToRay), 1e-6));
    float distance = max(length(l), MIN_DISTANCE);
    float a = s.roughness * s.roughness;
    float sphereA = saturate(a + radius / (2.0 * distance));
    float lineA = saturate(a + halfLength / distance);
    float normalisation = (a / sphereA) * (a / sphereA) * (a / lineA);
    float3 specular = specularGgx(s, l / distance, light.color / (distance * distance)) * normalisation;
    return (diffuse + specular) * window;
}

// How far the light rough glass lets through spreads, as a share of how far
// behind the glass it falls: frosted glass's, at smoothness 0, a patch this
// wide; clear glass's hardly at all.
static const float GLASS_SCATTER = 0.5;
// The farthest, in metres, glass is looked for round a point, and its light
// spreads.
static const float GLASS_REACH_METRES = 1.0;
// The coarsest of the glass shadow atlas's mips (src/glass.rs).
static const float GLASS_COARSEST = 5.0;

// The nearest pane round `uv` in the glass shadow atlas's mip `level`, by
// the 2x2 texels a bilinear read there takes in, and the least smoothness
// among them.
void glassAround(Texture2D light, Texture2D smoothness, float2 uv, uint level, out float nearest, out float smooth)
{
    uint2 size;
    uint mips;
    light.GetDimensions(level, size.x, size.y, mips);
    int2 corner = int2(floor(uv * float2(size) - 0.5));
    nearest = 1e30;
    smooth = 1.0;
    for (uint i = 0; i < 4; i++)
    {
        int3 at = int3(clamp(corner + int2(i % 2, i / 2), int2(0), int2(size) - 1), level);
        nearest = min(nearest, light.Load(at).a);
        smooth = min(smooth, smoothness.Load(at).r);
    }
}

// What glass lets through of a light to a point: each pane between leaves
// its own colours of the light, from the glass shadow atlas (src/glass.rs),
// and where there is none all of it passes. `uv` is the point in the atlas,
// inside the tile from `low` to `high`; `texelMetres` a texel's width there;
// `depth` the point's depth as the atlas writes it, and `depthMetres` one
// unit of it in metres. Rough glass spreads what it lets through wider the
// farther behind it the point lies, as frosted glass scatters it: read from
// the atlas's mip as wide as the spread, which averages the light let
// through over it, a soft, bright patch rather than the pane's outline.
float3 glassLight(Frame* frame, float2 uv, float2 low, float2 high, float texelMetres, float depth, float depthMetres)
{
    Texture2D light = textures[NonUniformResourceIndex(frame->glassShadow.x - 1)];
    Texture2D smoothness = textures[NonUniformResourceIndex(frame->glassShadow.y - 1)];
    // A little nearer the light than the point, so a pane does not filter the light falling on its own face.
    float before = depth - 2.0 * texelMetres / depthMetres;
    // Glass anywhere round the point its light could spread from: the nearest pane, and the roughest.
    float reach = clamp(ceil(log2(max(GLASS_REACH_METRES / texelMetres, 1.0))), 0.0, GLASS_COARSEST);
    float closest;
    float smooth;
    glassAround(light, smoothness, uv, uint(reach), closest, smooth);
    if (closest >= before)
    {
        return 1.0;
    }
    float behind = (depth - closest) * depthMetres;
    float spread = (1.0 - smooth) * GLASS_SCATTER * behind / texelMetres;
    float level = clamp(log2(max(spread, 1.0)), 0.0, GLASS_COARSEST);
    float near;
    float unused;
    glassAround(light, smoothness, uv, uint(ceil(level)), near, unused);
    if (near >= before)
    {
        return 1.0;
    }
    uint2 size;
    uint mips;
    light.GetDimensions(0, size.x, size.y, mips);
    // Kept inside the tile, by half a texel of the mip read.
    float2 inset = 0.5 * exp2(ceil(level)) / float2(size);
    return light.SampleLevel(samplers[SAMPLER_LINEAR_CLAMP], clamp(uv, low + inset, high - inset), level).rgb;
}

// Where a point falls in a local light's shadow views: the view, a spot
// light's own or the face of its cube the point lies towards, and the point
// there, pushed off its surface by a texel and a half against shadow acne,
// a texel being `texelMetres` wide. False where it falls in none.
bool localShadowAt(Frame* frame, Light light, float3 position, float3 normal, out uint view, out float3 ndc,
    out float texelMetres)
{
    float3 offset = position - light.position;
    float3 across = abs(offset);
    view = light.shadow;
    ndc = 0.0;
    float tanHalf = 1.0;
    if (light.kind == LIGHT_SPOT)
    {
        tanHalf = sqrt(max(1.0 - light.cosOuter * light.cosOuter, 0.0)) / max(light.cosOuter, 0.05);
    }
    else if (across.x >= across.y && across.x >= across.z)
    {
        view += offset.x >= 0.0 ? 0 : 1;
    }
    else if (across.y >= across.z)
    {
        view += offset.y >= 0.0 ? 2 : 3;
    }
    else
    {
        view += offset.z >= 0.0 ? 4 : 5;
    }
    texelMetres = 2.0 * length(offset) * tanHalf / float(LOCAL_SHADOW_TILE);
    float4 clip = transform(frame->localShadows.viewProj[view], float4(position + normal * texelMetres * 1.5, 1.0));
    if (clip.w <= 0.0)
    {
        return false;
    }
    ndc = clip.xyz / clip.w;
    return all(abs(ndc.xy) <= 1.0) && ndc.z <= 1.0;
}

// How much of a local light reaches `position` past what its shadow holds in
// the way: 1 for a light that casts none. Nine taps of hardware 2x2
// filtering soften the edge, kept inside the view.
float localShadow(Frame* frame, Light light, float3 position, float3 normal)
{
    if (light.shadow == NO_SHADOW || frame->localShadows.texture == NO_TEXTURE)
    {
        return 1.0;
    }
    uint view;
    float3 ndc;
    float texelMetres;
    if (!localShadowAt(frame, light, position, normal, view, ndc, texelMetres))
    {
        return 1.0;
    }
    float tile = float(LOCAL_SHADOW_TILE);
    float2 corner = float2(view % LOCAL_SHADOW_COLUMNS, view / LOCAL_SHADOW_COLUMNS) * tile;
    float2 low = (corner + 1.5) / float2(LOCAL_SHADOW_ATLAS);
    float2 high = (corner + tile - 1.5) / float2(LOCAL_SHADOW_ATLAS);
    float2 uv = (corner + (ndc.xy * 0.5 + 0.5) * tile) / float2(LOCAL_SHADOW_ATLAS);
    float2 texel = 1.0 / float2(LOCAL_SHADOW_ATLAS);
    Texture2D atlas = textures[NonUniformResourceIndex(frame->localShadows.texture)];
    float lit = 0.0;
    for (int y = -1; y <= 1; y++)
    {
        for (int x = -1; x <= 1; x++)
        {
            float2 at = clamp(uv + float2(x, y) * texel, low, high);
            lit += atlas.SampleCmpLevelZero(comparisonSamplers[COMPARISON_SAMPLER_SHADOW], at, ndc.z);
        }
    }
    return lit / 9.0;
}

// What glass lets through of a local light to a point (glassLight), white
// where no glass is in the way. The atlas keeps each pane's distance from
// the light, in metres.
float3 localGlass(Frame* frame, Light light, float3 position, float3 normal)
{
    if (light.shadow == NO_SHADOW || frame->glassShadow.x == 0)
    {
        return 1.0;
    }
    uint view;
    float3 ndc;
    float texelMetres;
    if (!localShadowAt(frame, light, position, normal, view, ndc, texelMetres))
    {
        return 1.0;
    }
    float tile = float(GLASS_LOCAL_TILE);
    float2 atlas = float2(GLASS_SHADOW_ATLAS);
    // Under the sun's part, laid out as the local shadow atlas lays its views.
    float2 corner = float2(view % LOCAL_SHADOW_COLUMNS, view / LOCAL_SHADOW_COLUMNS) * tile;
    corner.y += float(GLASS_SUN_SIZE);
    float2 uv = (corner + (ndc.xy * 0.5 + 0.5) * tile) / atlas;
    float glassTexel = texelMetres * float(LOCAL_SHADOW_TILE) / tile;
    float distance = length(position - light.position);
    return glassLight(frame, uv, corner / atlas, (corner + tile) / atlas, glassTexel, distance, 1.0);
}

// A local light as a custom shader's `light` takes it: the way to it, from
// its middle for an area or tube light, and its colour as it arrives,
// after distance, a spot's cone, its cookie and profile, its shadow and any
// glass in the way.
LightInput localLightInput(Light light, float3 position, float3 normal)
{
    float3 toLight = light.position - position;
    float distanceSquared = max(dot(toLight, toLight), 1e-4);
    toLight *= rsqrt(distanceSquared);
    float attenuation = rangeWindow(distanceSquared, light.range) / distanceSquared;
    float3 color = light.color;
    if (light.kind == LIGHT_SPOT)
    {
        attenuation *= smoothstep(light.cosOuter, light.cosInner, dot(-toLight, light.direction));
        if (light.cookie != NO_TEXTURE)
        {
            color *= cookieAt(light, -toLight);
        }
    }
    else if (light.kind == LIGHT_RECTANGLE || light.kind == LIGHT_DISC)
    {
        attenuation *= saturate(dot(-toLight, light.direction));
    }
    if (light.profile != NO_TEXTURE)
    {
        attenuation *= profileAt(light, -toLight);
    }
    float shadow = localShadow(push.frame, light, position, normal);
    color *= attenuation * shadow * localGlass(push.frame, light, position, normal);
    LightInput input = { toLight, color, float3(0.0) };
    return input;
}

float3 shadeLocalLight(Surface s, Light light, float3 position)
{
    float shadow = localShadow(push.frame, light, position, s.normal);
    if (shadow <= 0.0)
    {
        return 0.0;
    }
    float3 through = shadow * localGlass(push.frame, light, position, s.normal);
    switch (light.kind)
    {
    case LIGHT_RECTANGLE:
        return shadeRectangleLight(s, light, position) * through;
    case LIGHT_DISC:
        return shadeDiscLight(s, light, position) * through;
    case LIGHT_TUBE:
        return shadeTubeLight(s, light, position) * through;
    default:
        return shadePunctualLight(s, light, position) * through;
    }
}

// How far a light's shape reaches from its position, which its range is
// measured from: what the froxels it lights are found by.
float lightExtent(Light light)
{
    switch (light.kind)
    {
    case LIGHT_RECTANGLE:
        return length(light.size);
    case LIGHT_DISC:
        return light.size.x;
    case LIGHT_TUBE:
        return light.size.x + light.size.y;
    default:
        return 0.0;
    }
}

// The sun's cookie where its light reaches `position`, tiled over the world.
float3 sunCookieAt(Frame* frame, float3 position)
{
    Lighting lighting = frame->lighting;
    if (lighting.sunCookie == NO_TEXTURE)
    {
        return 1.0;
    }
    float2 uv = float2(dot(position, lighting.sunRight), -dot(position, lighting.sunUp)) / max(lighting.sunCookieSize, 1e-3);
    return textures[lighting.sunCookie].Sample(samplers[SAMPLER_LINEAR_REPEAT], uv).rgb;
}
