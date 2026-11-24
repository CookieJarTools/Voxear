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

// Points on a disc spread evenly by the golden angle, turned by `angle`.
float2 vogelDisc(uint index, uint count, float angle)
{
    float radius = sqrt((float(index) + 0.5) / float(count));
    float theta = float(index) * 2.39996323 + angle;
    return radius * float2(cos(theta), sin(theta));
}

// Whether a light's shadow is read from the view's kept atlas of what stands
// still (src/shadow_cache.rs): it keeps one, and nothing moving was drawn
// for it this frame over a copy in the atlas.
bool localShadowKept(Frame* frame, uint slot)
{
    uint cached = frame->localShadows.cached;
    if (frame->localShadows.cache == NO_TEXTURE || ((cached >> slot) & 1) == 0 || ((cached >> (KEPT_MOVING_SHIFT + slot)) & 1) != 0)
    {
        return false;
    }
    return !lampListDrawn(slot);
}

// Few taps, turned each frame and pixel, as TAA averages them: an area
// light's soft shadow.
static const uint LOCAL_PCSS_BLOCKER_TAPS = 12;
static const uint LOCAL_PCSS_FILTER_TAPS = 16;
// The widest, in texels of a view, its soft edge grows, which its search
// for blockers keeps within too.
static const float LOCAL_PCSS_MAX_TEXELS = 24.0;

// How far from a light, along its view's axis, a depth its view holds lies.
float localShadowDistance(Light light, float depth)
{
    return light.shadowNear / max(1.0 - depth * (light.shadowFar - light.shadowNear) / light.shadowFar, 1e-6);
}

// A local light's shape as a point at `position` sees it, across the way
// from the light to it: two half axes, in metres. A rectangle's width and
// height, a disc's radius both ways, a tube's half length and its radius
// across it, a point or spot light's radius both ways.
void localLightAcross(Light light, float3 position, out float3 u, out float3 v)
{
    float3 way = normalizeOr(position - light.position, light.direction);
    float3 up = lightUp(light);
    if (light.kind == LIGHT_RECTANGLE)
    {
        u = light.right * light.size.x;
        v = up * light.size.y;
    }
    else if (light.kind == LIGHT_DISC)
    {
        u = light.right * light.size.x;
        v = up * light.size.x;
    }
    else if (light.kind == LIGHT_TUBE)
    {
        u = light.right * (light.size.x + light.size.y);
        v = normalizeOr(cross(way, light.right), up) * light.size.y;
    }
    else
    {
        float3 side = normalizeOr(cross(way, abs(way.y) < 0.9 ? float3(0.0, 1.0, 0.0) : float3(1.0, 0.0, 0.0)), up);
        u = side * light.shadowRadius;
        v = cross(way, side) * light.shadowRadius;
    }
    u -= dot(u, way) * way;
    v -= dot(v, way) * way;
}

// Where in the atlas a point falls in local shadow view `view`.
float2 localAtlasUv(Frame* frame, uint view, float3 position)
{
    float4 clip = transform(frame->localShadows.viewProj[view], float4(position, 1.0));
    float2 corner = float2(view % LOCAL_SHADOW_COLUMNS, view / LOCAL_SHADOW_COLUMNS) * float(LOCAL_SHADOW_TILE);
    return (corner + (clip.xy / max(clip.w, 1e-6) * 0.5 + 0.5) * float(LOCAL_SHADOW_TILE)) / float2(LOCAL_SHADOW_ATLAS);
}

// A shaped light's shadow at `uv` in its view `view`, at a depth of
// `depth`: the blockers within the light's sight of the point found first,
// then a filter as large as the light seen past them, its shape and turn
// as the light's, so a tube's shadow softens along its length and stays
// sharper across it. The taps are turned each frame and pixel.
float localSoftShadow(Frame* frame, Light light, Texture2D atlas, uint view, float2 uv, float depth, float2 low,
    float2 high, float3 position)
{
    float2 texelUv = 1.0 / float2(LOCAL_SHADOW_ATLAS);
    float receiver = localShadowDistance(light, depth);
    float angle = frac(52.9829189 * frac(dot(position.xz * 37.0 + position.y * 11.0, float2(0.06711056, 0.00583715)))) * 2.0 * PI;
    angle += float(frame->frameIndex % 64) * 2.39996323;
    // The light's half axes laid over the atlas at the point, as far as each reaches past a blocker as far from the
    // point as the light is: scaled by how much farther the point is than the blocker.
    float3 u;
    float3 v;
    localLightAcross(light, position, u, v);
    float2 base = localAtlasUv(frame, view, position);
    float2 alongU = localAtlasUv(frame, view, position + u) - base;
    float2 alongV = localAtlasUv(frame, view, position + v) - base;
    float widest = max(length(alongU / texelUv), length(alongV / texelUv));
    // A blocker as near the light as its shape reaches still lies within the light's sight of the point this far.
    float nearest = max(light.shadowNear, light.shadowRadius);
    float search = min(max(receiver - nearest, 0.0) / nearest, LOCAL_PCSS_MAX_TEXELS / max(widest, 1e-4));
    float blockers = 0.0;
    float blockerDistance = 0.0;
    for (uint i = 0; i < LOCAL_PCSS_BLOCKER_TAPS; i++)
    {
        float2 disc = vogelDisc(i, LOCAL_PCSS_BLOCKER_TAPS, angle);
        float2 at = clamp(uv + (alongU * disc.x + alongV * disc.y) * search + disc * texelUv, low, high);
        float found = atlas.SampleLevel(samplers[SAMPLER_NEAREST_CLAMP], at, 0.0).r;
        if (found < depth)
        {
            blockers += 1.0;
            blockerDistance += localShadowDistance(light, found);
        }
    }
    // Nothing round it is in the way, or everything is: wholly lit or wholly shaded, with no edge to soften.
    if (blockers == 0.0)
    {
        return 1.0;
    }
    if (blockers == float(LOCAL_PCSS_BLOCKER_TAPS))
    {
        return 0.0;
    }
    // The light seen past the blockers: as large as it is over how far they stand from it.
    float blocker = max(blockerDistance / blockers, 1e-3);
    float spread = min(max(receiver - blocker, 0.0) / blocker, LOCAL_PCSS_MAX_TEXELS / max(widest, 1e-4));
    float lit = 0.0;
    for (uint i = 0; i < LOCAL_PCSS_FILTER_TAPS; i++)
    {
        float2 disc = vogelDisc(i, LOCAL_PCSS_FILTER_TAPS, angle);
        float2 at = clamp(uv + (alongU * disc.x + alongV * disc.y) * spread + disc * texelUv, low, high);
        lit += atlas.SampleCmpLevelZero(comparisonSamplers[COMPARISON_SAMPLER_SHADOW], at, depth);
    }
    return lit / float(LOCAL_PCSS_FILTER_TAPS);
}

// A lamp's virtual shadow pages (src/lamp_vsm.rs): a point's page of a level of one of its views, or of the next
// coarser level drawn there, and its slot's corner in the pool.
struct LampTap
{
    bool found;
    uint mip;
    int2 page;
    float2 corner;
};

// The widest, in texels of a level, a soft shadow's taps reach from the page's middle: short of the border, which a
// tap's 2x2 filter reaches a texel past.
static const float LAMP_SOFT_TEXELS = 7.0;

// Whether a lamp's shadow of what stands still is read from its pages: lamps have them, and its slot is kept (it
// stands still, and its coarsest pages are drawn).
bool lampPaged(Frame* frame, uint slot)
{
    return frame->lampVsm.pool != NO_TEXTURE && ((frame->localShadows.cached >> slot) & 1) != 0;
}

// Whether anything moving is drawn into a lamp's views of the atlas this frame, over its pages.
bool lampMoving(Frame* frame, uint slot)
{
    return ((frame->localShadows.cached >> (KEPT_MOVING_SHIFT + slot)) & 1) != 0
        || lampListDrawn(slot);
}

// How wide a texel of level `mip` of a lamp's view is `distance` from it, in metres.
float lampTexelMetres(Frame* frame, uint slot, float distance, uint mip)
{
    return 2.0 * distance * frame->lampVsm.lights[slot].z / float(LAMP_VSM_FINEST >> mip);
}

// The finest level a point `distance` from a lamp asks for: the one whose texels are no wider than its pixel, and no
// coarser than the atlas's (markLampsMain).
uint lampMipFor(Frame* frame, uint slot, float3 position, float distance)
{
    float pixel = screenDistance(frame, max(length(position - frame->cameraPosition), frame->near)) / frame->lodScale;
    float texel = max(lampTexelMetres(frame, slot, distance, 0), 1e-6);
    float mip = floor(log2(max(pixel / texel, 1.0)) + frame->lampVsm.bias);
    return uint(clamp(mip, 0.0, float(LAMP_VSM_ASKED)));
}

// The level whose texels make `width`, across a view from 0 to 1, about LAMP_SOFT_TEXELS of them, no finer than
// `finest`.
uint lampMipForWidth(float width, uint finest)
{
    float mip = ceil(log2(max(width * float(LAMP_VSM_FINEST) / LAMP_SOFT_TEXELS, 1.0)));
    return uint(clamp(max(mip, float(finest)), 0.0, float(LAMP_VSM_MIPS - 1)));
}

// Where a point falls across a lamp's view `view`, from 0 to 1 either way, and its depth there.
float3 lampViewAt(Frame* frame, uint view, float3 position)
{
    float4 clip = transform(frame->localShadows.viewProj[view], float4(position, 1.0));
    float3 ndc = clip.xyz / max(clip.w, 1e-6);
    return float3(ndc.xy * 0.5 + 0.5, ndc.z);
}

LampTap lampTap(Frame* frame, uint slot, uint face, float2 uv, uint mip)
{
    LampTap tap;
    tap.found = false;
    tap.mip = mip;
    tap.page = 0;
    tap.corner = 0.0;
    for (uint k = mip; k < LAMP_VSM_MIPS; k++)
    {
        int across = int((LAMP_VSM_FINEST / VSM_PAGE) >> k);
        int2 page = clamp(int2(floor(uv * float(across))), 0, across - 1);
        uint entry = frame->lampVsm.table[lampPageIndex(slot, face, k, page)];
        if (entry == 0)
        {
            continue;
        }
        uint held = entry - 1;
        tap.found = true;
        tap.mip = k;
        tap.page = page;
        tap.corner = float2(held % LAMP_VSM_POOL_SLOTS, held / LAMP_VSM_POOL_SLOTS) * float(VSM_SLOT);
        return tap;
    }
    return tap;
}

// Where `uv` across a lamp's view falls in the pool, kept within the slot of the tap's page, whose border reaches as
// far as any filter centred in the page.
float2 lampPoolUv(LampTap tap, float2 uv)
{
    float side = float(LAMP_VSM_FINEST >> tap.mip);
    float2 within = uv * side - float2(tap.page) * float(VSM_PAGE) + float(VSM_BORDER);
    return (tap.corner + clamp(within, 1.0, float(VSM_SLOT) - 1.0)) / float(LAMP_VSM_POOL);
}

// A shaped lamp's soft shadow from its pages, as localSoftShadow reads the atlas: blockers searched for at a level
// whose texels keep the search within a page's border, then a filter as large as the light seen past them at its own.
float lampSoftShadow(Frame* frame, Light light, uint slot, uint face, uint view, float3 position, float3 normal,
    uint finest)
{
    Texture2D pool = textures[frame->lampVsm.pool];
    float distance = length(position - light.position);
    float3 u;
    float3 v;
    localLightAcross(light, position, u, v);
    float3 base = lampViewAt(frame, view, position);
    float2 alongU = lampViewAt(frame, view, position + u).xy - base.xy;
    float2 alongV = lampViewAt(frame, view, position + v).xy - base.xy;
    float widest = max(max(length(alongU), length(alongV)), 1e-6);
    // As far across the view as the atlas's soft edges reach.
    float most = LOCAL_PCSS_MAX_TEXELS / float(LOCAL_SHADOW_TILE);
    float receiver = localShadowDistance(light, base.z);
    float nearest = max(light.shadowNear, light.shadowRadius);
    float search = min(max(receiver - nearest, 0.0) / nearest, most / widest);
    LampTap centre = lampTap(frame, slot, face, base.xy, lampMipForWidth(widest * search, finest));
    if (!centre.found)
    {
        return 1.0;
    }
    float3 at = lampViewAt(frame, view, position + normal * lampTexelMetres(frame, slot, distance, centre.mip) * 1.5);
    float texel = 1.0 / float(LAMP_VSM_FINEST >> centre.mip);
    float angle = frac(52.9829189 * frac(dot(position.xz * 37.0 + position.y * 11.0, float2(0.06711056, 0.00583715)))) * 2.0 * PI;
    angle += float(frame->frameIndex % 64) * 2.39996323;
    float blockers = 0.0;
    float blockerDistance = 0.0;
    for (uint i = 0; i < LOCAL_PCSS_BLOCKER_TAPS; i++)
    {
        float2 disc = vogelDisc(i, LOCAL_PCSS_BLOCKER_TAPS, angle);
        float2 tap = at.xy + (alongU * disc.x + alongV * disc.y) * search + disc * texel;
        float found = pool.SampleLevel(samplers[SAMPLER_NEAREST_CLAMP], lampPoolUv(centre, tap), 0.0).r;
        if (found < at.z)
        {
            blockers += 1.0;
            blockerDistance += localShadowDistance(light, found);
        }
    }
    if (blockers == 0.0)
    {
        return 1.0;
    }
    if (blockers == float(LOCAL_PCSS_BLOCKER_TAPS))
    {
        return 0.0;
    }
    float blocker = max(blockerDistance / blockers, 1e-3);
    float spread = min(max(receiver - blocker, 0.0) / blocker, most / widest);
    LampTap filter = lampTap(frame, slot, face, base.xy, lampMipForWidth(widest * spread, finest));
    if (!filter.found)
    {
        return 1.0;
    }
    float3 filterAt = lampViewAt(frame, view, position + normal * lampTexelMetres(frame, slot, distance, filter.mip) * 1.5);
    float filterTexel = 1.0 / float(LAMP_VSM_FINEST >> filter.mip);
    float lit = 0.0;
    for (uint i = 0; i < LOCAL_PCSS_FILTER_TAPS; i++)
    {
        float2 disc = vogelDisc(i, LOCAL_PCSS_FILTER_TAPS, angle);
        float2 tap = filterAt.xy + (alongU * disc.x + alongV * disc.y) * spread + disc * filterTexel;
        lit += pool.SampleCmpLevelZero(comparisonSamplers[COMPARISON_SAMPLER_SHADOW], lampPoolUv(filter, tap), filterAt.z);
    }
    return lit / float(LOCAL_PCSS_FILTER_TAPS);
}

// How much of a lamp's light its pages let through to `position`, seen in view `view`: 3x3 taps of hardware 2x2
// filtering at the finest level drawn round the point, pushed off its surface by a texel and a half of it, or the soft
// shadow. Lit where no page is drawn.
float lampShadow(Frame* frame, Light light, uint view, float3 position, float3 normal, bool soft)
{
    uint slot = light.shadow / LOCAL_SHADOW_SLOT_VIEWS;
    uint face = view - light.shadow;
    float distance = length(position - light.position);
    uint finest = lampMipFor(frame, slot, position, distance);
    if (soft && light.shadowRadius > 0.0)
    {
        return lampSoftShadow(frame, light, slot, face, view, position, normal, finest);
    }
    LampTap tap = lampTap(frame, slot, face, lampViewAt(frame, view, position).xy, finest);
    if (!tap.found)
    {
        return 1.0;
    }
    float3 at = lampViewAt(frame, view, position + normal * lampTexelMetres(frame, slot, distance, tap.mip) * 1.5);
    float texel = 1.0 / float(LAMP_VSM_FINEST >> tap.mip);
    Texture2D pool = textures[frame->lampVsm.pool];
    float lit = 0.0;
    for (int y = -1; y <= 1; y++)
    {
        for (int x = -1; x <= 1; x++)
        {
            float2 uv = lampPoolUv(tap, at.xy + float2(x, y) * texel);
            lit += pool.SampleCmpLevelZero(comparisonSamplers[COMPARISON_SAMPLER_SHADOW], uv, at.z);
        }
    }
    return lit / 9.0;
}

// How much of a local light reaches `position` past what its atlas view `view` holds in the way, at `ndc`.
float localAtlasShadow(Frame* frame, Light light, uint texture, uint view, float3 ndc, float3 position, bool soft)
{
    float tile = float(LOCAL_SHADOW_TILE);
    float2 corner = float2(view % LOCAL_SHADOW_COLUMNS, view / LOCAL_SHADOW_COLUMNS) * tile;
    float2 low = (corner + 1.5) / float2(LOCAL_SHADOW_ATLAS);
    float2 high = (corner + tile - 1.5) / float2(LOCAL_SHADOW_ATLAS);
    float2 uv = (corner + (ndc.xy * 0.5 + 0.5) * tile) / float2(LOCAL_SHADOW_ATLAS);
    float2 texel = 1.0 / float2(LOCAL_SHADOW_ATLAS);
    Texture2D atlas = textures[NonUniformResourceIndex(texture)];
    if (soft && light.shadowRadius > 0.0)
    {
        return localSoftShadow(frame, light, atlas, view, uv, ndc.z, low, high, position);
    }
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

// How much of a local light reaches `position` past what its shadow holds in
// the way: 1 for a light that casts none. Nine taps of hardware 2x2
// filtering soften the edge, kept inside the view. A light with a shape or
// a radius, with `soft`, softens it as wide as the light is: percentage-closer
// soft shadows (Fernando 2005), so shadows stay sharp where things touch
// and soften away from them. A lamp read from its virtual shadow pages takes
// the darker of them and what moves, drawn into the atlas this frame.
float localShadow(Frame* frame, Light light, float3 position, float3 normal, bool soft = true)
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
    uint slot = light.shadow / LOCAL_SHADOW_SLOT_VIEWS;
    if (lampPaged(frame, slot))
    {
        float lit = lampShadow(frame, light, view, position, normal, soft);
        if (lit <= 0.0 || !lampMoving(frame, slot))
        {
            return lit;
        }
        return min(lit, localAtlasShadow(frame, light, frame->localShadows.texture, view, ndc, position, soft));
    }
    bool kept = localShadowKept(frame, slot);
    uint texture = kept ? frame->localShadows.cache : frame->localShadows.texture;
    return localAtlasShadow(frame, light, texture, view, ndc, position, soft);
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
LightInput localLightInput(Light light, float3 position, float3 normal, bool soft = true)
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
    float shadow = localShadow(push.frame, light, position, normal, soft);
    color *= attenuation * shadow * localGlass(push.frame, light, position, normal);
    LightInput input = { toLight, color, float3(0.0) };
    return input;
}

float3 shadeLocalLight(Surface s, Light light, float3 position, bool soft = true)
{
    float shadow = localShadow(push.frame, light, position, s.normal, soft);
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
    // Tiled from the level's middle, so the pattern stays where it is as the world's middle moves.
    float3 level = levelPosition(frame, position, 4096.0);
    float2 uv = float2(dot(level, lighting.sunRight), -dot(level, lighting.sunUp)) / max(lighting.sunCookieSize, 1e-3);
    return textures[lighting.sunCookie].Sample(samplers[SAMPLER_LINEAR_REPEAT], uv).rgb;
}
