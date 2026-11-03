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

float3 shadeLocalLight(Surface s, Light light, float3 position)
{
    switch (light.kind)
    {
    case LIGHT_RECTANGLE:
        return shadeRectangleLight(s, light, position);
    case LIGHT_DISC:
        return shadeDiscLight(s, light, position);
    case LIGHT_TUBE:
        return shadeTubeLight(s, light, position);
    default:
        return shadePunctualLight(s, light, position);
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
