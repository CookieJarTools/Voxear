// Ground-truth ambient occlusion (GTAO, Jimenez et al. 2016) as three
// fullscreen passes over the depth the depth passes drew, before the opaque
// passes shade: the occlusion itself, a spatial denoise, and a temporal pass
// that blends it with last frame's. The opaque passes read the result
// (occlusion.slang). The horizon search follows Intel's XeGTAO. See
// src/ambient_occlusion.rs for the order.
//
// The first two passes may run at half resolution, push.level 1. Each of
// their pixels then stands for one full-resolution pixel of its 2x2, a
// different one each frame, and the temporal pass, always at full
// resolution, takes each pixel's occlusion from the nearest of them that lie
// on its surface.

#include "common.slang"
#include "ddgi_sample.slang"
#include "reflection_probes.slang"

// The largest search radius on screen, in pixels at 1080p: wider searches
// thrash the texture cache for little change.
static const float MAX_SCREEN_RADIUS = 256.0;
// Searches start this many pixels out, so a pixel never occludes itself.
static const float MIN_SAMPLE_PIXELS = 1.3;
// The last part of the radius over which occluders fade out, so the edge of
// the search does not show as a line.
static const float FALLOFF = 0.615;
// How much of last frame's result the temporal pass keeps.
static const float HISTORY = 0.9;
// How far apart two LODs of a mesh may lie while they crossfade, in the
// pixels of error past which a LOD is too coarse to draw (lodThreshold),
// with room for the camera moving closer during the fade.
static const float LOD_FADE_GAP = 2.0;
static const float HALF_PI = PI * 0.5;

struct AoOut
{
    float4 position : SV_Position;
    float2 uv : TEXCOORD0;
};

[shader("vertex")]
AoOut aoVertex(uint vertexId : SV_VertexID)
{
    AoOut o;
    float2 ndc = fullscreenNdc(vertexId);
    o.position = float4(ndc, 0.0, 1.0);
    o.uv = ndc * 0.5 + 0.5;
    return o;
}

float loadIn(uint texture, int2 pixel, int2 size)
{
    return textures[NonUniformResourceIndex(texture)].Load(int3(clamp(pixel, int2(0), size - 1), 0)).r;
}

float loadDepth(uint depthTexture, int2 pixel)
{
    return loadIn(depthTexture, pixel, int2(push.frame->viewport));
}

// The size of the images the first two passes write.
int2 searchSize()
{
    int2 full = int2(push.frame->viewport);
    return (full + (1 << push.level) - 1) >> push.level;
}

float loadSearch(uint texture, int2 pixel)
{
    return loadIn(texture, pixel, searchSize());
}

// Which pixel of each 2x2 the half-resolution passes stand for this frame:
// the diagonal first, so two frames already cover it evenly.
int2 searchOffset()
{
    if (push.level == 0)
    {
        return int2(0);
    }
    static const int2 order[4] = { int2(0, 0), int2(1, 1), int2(1, 0), int2(0, 1) };
    return order[push.frame->frameIndex % 4];
}

// The full-resolution pixel a pixel of the first two passes stands for.
int2 fullPixel(int2 pixel)
{
    int2 full = pixel * (1 << push.level) + searchOffset();
    return clamp(full, int2(0), int2(push.frame->viewport) - 1);
}

// Where a pixel's centre lies in view space at its depth. Depth is infinite
// reverse-Z, so view depth is near / depth; TAA's jitter is taken out, so
// neighbours line up as the surface does.
float3 viewPosition(float2 pixel, float depth)
{
    Frame* frame = push.frame;
    float2 size = float2(frame->viewport);
    float2 ndc = pixel / size * 2.0 - 1.0 - frame->jitter * 2.0 / size;
    float z = frame->near / max(depth, 1e-7);
    return float3(ndc * frame->post.aoUnproject * z, z);
}

float3 viewPositionAt(uint depthTexture, int2 pixel)
{
    return viewPosition(float2(pixel) + 0.5, loadDepth(depthTexture, pixel));
}

// The surface's normal from its depth: across each axis, the neighbour
// nearer in depth, so normals at silhouettes come from the surface they are
// on rather than bridging the gap to what lies behind.
float3 viewNormal(uint depthTexture, int2 pixel, float3 centre)
{
    float3 left = viewPositionAt(depthTexture, pixel + int2(-1, 0));
    float3 right = viewPositionAt(depthTexture, pixel + int2(1, 0));
    float3 up = viewPositionAt(depthTexture, pixel + int2(0, -1));
    float3 down = viewPositionAt(depthTexture, pixel + int2(0, 1));
    float3 across = abs(centre.z - left.z) < abs(right.z - centre.z) ? centre - left : right - centre;
    float3 along = abs(centre.z - up.z) < abs(down.z - centre.z) ? centre - up : down - centre;
    float3 normal = normalize(cross(across, along));
    // Facing the camera, whichever way round the cross product came out.
    return dot(normal, centre) > 0.0 ? -normal : normal;
}

float fastAcos(float x)
{
    float result = (-0.156583 * abs(x) + HALF_PI) * sqrt(1.0 - abs(x));
    return x >= 0.0 ? result : PI - result;
}

// Interleaved gradient noise, shifted each frame along the golden ratio so
// the temporal pass sees new directions every frame.
float2 aoNoise(int2 pixel, uint frameIndex)
{
    float2 p = float2(pixel);
    float slice = frac(52.9829189 * frac(dot(p, float2(0.06711056, 0.00583715))));
    float along = frac(52.9829189 * frac(dot(p + float2(47.0, 17.0), float2(0.00583715, 0.06711056))));
    float frame = float(frameIndex % 1024);
    return frac(float2(slice, along) + frame * float2(0.618033989, 0.754877666));
}

// ---------------------------------------------------------------- GTAO

// For each of a few directions (slices) around the pixel, the highest
// horizon on either side within the radius, then the cosine-weighted share
// of the hemisphere above those horizons, integrated in closed form.
// textures: x depth, y the surface: normal, and whether its LODs crossfade.
// Writes visibility, 1 where nothing occludes.
[shader("fragment")]
float4 gtaoFragment(AoOut input) : SV_Target
{
    Frame* frame = push.frame;
    uint depthTexture = push.textures.x;
    int2 pixel = fullPixel(int2(input.position.xy));
    float depth = loadDepth(depthTexture, pixel);
    if (depth <= 0.0)
    {
        return 1.0;
    }
    float3 centre = viewPosition(float2(pixel) + 0.5, depth);
    float3 normal = viewNormal(depthTexture, pixel, centre);
    // While two LODs crossfade, dithered, each pixel's depth is one or the other, and the simpler lies a little
    // inside the other: its pixels found the other's in front as occluders, and the normals from depth tilted
    // every way, so a chrome ball was blotched dark for as long as each fade lasted. There, the surface's own
    // normal, and the horizons looked for from just past the gap between them.
    float4 surface = textures[NonUniformResourceIndex(push.textures.y)].Load(int3(pixel, 0));
    if (surface.a < 0.5)
    {
        normal = normalize(transform(frame->view, float4(octDecode(surface.rg * 2.0 - 1.0), 0.0)).xyz);
        normal = dot(normal, centre) > 0.0 ? -normal : normal;
        centre += normal * (LOD_FADE_GAP * frame->lodThreshold * centre.z / frame->lodScale);
    }
    float3 toView = normalize(-centre);

    float radius = max(frame->post.aoRadius, 1e-3);
    float2 size = float2(frame->viewport);
    // View-space units a pixel spans at depth 1, along x and y.
    float2 pixelSpan = frame->post.aoUnproject * 2.0 / size;
    float screenRadius = min(radius / (abs(pixelSpan.y) * centre.z), MAX_SCREEN_RADIUS * size.y / 1080.0);
    if (screenRadius < MIN_SAMPLE_PIXELS)
    {
        return 1.0;
    }
    float falloffRange = FALLOFF * radius;
    float falloffMul = -1.0 / falloffRange;
    float falloffAdd = (radius - falloffRange) / falloffRange + 1.0;

    float2 noise = aoNoise(pixel, frame->frameIndex);
    uint slices = max(frame->post.aoSlices, 1);
    uint steps = max(frame->post.aoSteps, 1);
    float visibility = 0.0;
    for (uint slice = 0; slice < slices; slice++)
    {
        float angle = (float(slice) + noise.x) * PI / float(slices);
        float2 omega = float2(cos(angle), sin(angle));
        // The slice's direction in view space, as a step along omega on screen moves.
        float3 direction = normalize(float3(omega * pixelSpan, 0.0));
        float3 orthoDirection = direction - dot(direction, toView) * toView;
        float3 axis = normalize(cross(orthoDirection, toView));
        float3 projectedNormal = normal - axis * dot(normal, axis);
        float projectedLength = length(projectedNormal);
        float cosNormal = saturate(dot(projectedNormal, toView) / max(projectedLength, 1e-5));
        float n = sign(dot(orthoDirection, projectedNormal)) * fastAcos(cosNormal);

        // Start each side's horizon at the surface's own tangent.
        float lowCos0 = cos(n + HALF_PI);
        float lowCos1 = cos(n - HALF_PI);
        float horizonCos0 = lowCos0;
        float horizonCos1 = lowCos1;
        for (uint taken = 0; taken < steps; taken++)
        {
            // Squared, so samples crowd near the pixel where contact matters most.
            float t = (float(taken) + noise.y) / float(steps);
            float2 offset = round(omega * (t * t * screenRadius + MIN_SAMPLE_PIXELS));
            float3 delta0 = viewPositionAt(depthTexture, pixel + int2(offset)) - centre;
            float3 delta1 = viewPositionAt(depthTexture, pixel - int2(offset)) - centre;
            float length0 = length(delta0);
            float length1 = length(delta1);
            float cos0 = dot(delta0, toView) / max(length0, 1e-6);
            float cos1 = dot(delta1, toView) / max(length1, 1e-6);
            horizonCos0 = max(horizonCos0, lerp(lowCos0, cos0, saturate(length0 * falloffMul + falloffAdd)));
            horizonCos1 = max(horizonCos1, lerp(lowCos1, cos1, saturate(length1 * falloffMul + falloffAdd)));
        }

        // XeGTAO's small fudge, so slices nearly edge-on to the normal still count a little.
        projectedLength = lerp(projectedLength, 1.0, 0.05);
        float h0 = n + clamp(-fastAcos(horizonCos1) - n, -HALF_PI, HALF_PI);
        float h1 = n + clamp(fastAcos(horizonCos0) - n, -HALF_PI, HALF_PI);
        float arc0 = cosNormal + 2.0 * h0 * sin(n) - cos(2.0 * h0 - n);
        float arc1 = cosNormal + 2.0 * h1 * sin(n) - cos(2.0 * h1 - n);
        visibility += projectedLength * (arc0 + arc1) * 0.25;
    }
    return saturate(visibility / float(slices));
}

// ---------------------------------------------------------------- Denoise

// A 5x5 blur (3x3 at half resolution) that only mixes pixels lying close to this pixel's plane, so
// occlusion does not bleed across edges onto surfaces in front or behind.
// textures: x GTAO's visibility, y depth.
[shader("fragment")]
float4 aoDenoiseFragment(AoOut input) : SV_Target
{
    uint depthTexture = push.textures.y;
    int2 cell = int2(input.position.xy);
    int2 pixel = fullPixel(cell);
    float depth = loadDepth(depthTexture, pixel);
    if (depth <= 0.0)
    {
        return 1.0;
    }
    float3 centre = viewPosition(float2(pixel) + 0.5, depth);
    float3 normal = viewNormal(depthTexture, pixel, centre);
    // At half resolution a 3x3 covers what a 5x5 does at full, so the blur
    // stays as wide on screen; its neighbours lie twice as far across the
    // surface, so they may stray twice as far from its plane.
    int reach = 2 >> push.level;
    float tolerance = 0.02 * float(1 << push.level) * centre.z;
    float sum = 0.0;
    float weight = 0.0;
    for (int y = -reach; y <= reach; y++)
    {
        for (int x = -reach; x <= reach; x++)
        {
            int2 at = cell + int2(x, y);
            float distance = abs(dot(viewPositionAt(depthTexture, fullPixel(at)) - centre, normal));
            float w = saturate(1.0 - distance / tolerance);
            sum += loadSearch(push.textures.x, at) * w;
            weight += w;
        }
    }
    return sum / max(weight, 1e-4);
}

// ---------------------------------------------------------------- Temporal

// This frame's denoised occlusion at a full-resolution pixel, with the range
// of what lies around it for clamping last frame's. At full resolution that
// is the pixel and its 3x3. At half resolution it is a blend of the four
// searched pixels nearest it, each by how near it is and how close its depth
// is to this pixel's, so occlusion does not cross from one surface onto the
// next; when none of them lie on this surface, the one nearest in depth.
float upsample(int2 pixel, float depth, out float low, out float high)
{
    uint denoised = push.textures.x;
    if (push.level == 0)
    {
        float current = loadSearch(denoised, pixel);
        low = current;
        high = current;
        for (int y = -1; y <= 1; y++)
        {
            for (int x = -1; x <= 1; x++)
            {
                float value = loadSearch(denoised, pixel + int2(x, y));
                low = min(low, value);
                high = max(high, value);
            }
        }
        return current;
    }

    float z = viewPosition(float2(pixel) + 0.5, depth).z;
    float tolerance = 0.05 * z;
    int2 first = (pixel - searchOffset()) >> 1;
    float sum = 0.0;
    float weight = 0.0;
    float nearest = 1.0;
    float nearestGap = 1e30;
    low = 1.0;
    high = 0.0;
    for (int y = 0; y <= 1; y++)
    {
        for (int x = 0; x <= 1; x++)
        {
            int2 cell = first + int2(x, y);
            int2 at = fullPixel(cell);
            float value = loadSearch(denoised, cell);
            float gap = abs(viewPositionAt(push.textures.z, at).z - z);
            int2 apart = abs(at - pixel);
            float w = saturate(1.0 - gap / tolerance) / float(1 + apart.x + apart.y);
            sum += value * w;
            weight += w;
            if (gap < nearestGap)
            {
                nearestGap = gap;
                nearest = value;
            }
            low = min(low, value);
            high = max(high, value);
        }
    }
    return weight > 1e-4 ? sum / weight : nearest;
}


// Blends with last frame's result, found where this pixel's surface was on
// screen then, as TAA does. Velocity is not drawn yet this early in the
// frame, so the camera's motion alone moves it; what moved on its own is
// kept from smearing by clamping last frame's value to this frame's
// neighbourhood. textures: x the denoised visibility, y last frame's result,
// z depth.
[shader("fragment")]
float4 aoTemporalFragment(AoOut input) : SV_Target
{
    Frame* frame = push.frame;
    int2 pixel = int2(input.position.xy);
    float depth = loadDepth(push.textures.z, pixel);
    if (depth <= 0.0)
    {
        return 1.0;
    }
    float low;
    float high;
    float current = upsample(pixel, depth, low, high);
    if (frame->post.aoReset != 0)
    {
        return current;
    }
    float4 world = transform(frame->invViewProj, float4(input.uv * 2.0 - 1.0, depth, 1.0));
    float4 previous = transform(frame->prevViewProj, float4(world.xyz / world.w, 1.0));
    float2 previousUv = previous.xy / previous.w * 0.5 + 0.5;
    if (previous.w <= 0.0 || any(previousUv < 0.0) || any(previousUv > 1.0))
    {
        return current;
    }
    float history = textures[NonUniformResourceIndex(push.textures.y)].SampleLevel(samplers[SAMPLER_LINEAR_CLAMP], previousUv, 0.0).r;
    return lerp(current, clamp(history, low, high), HISTORY);
}

// ---------------------------------------------------------------- Contact shadows

// Contact shadows: from each pixel, a short march towards the sun through
// the depth buffer, for the small shadows where things touch that the
// shadow map is too coarse to hold. A step that lands behind something in
// the depth buffer, but no more than its thickness behind, is in shadow.
// textures: x depth. Writes 1 where nothing is found, 0 in shadow.
[shader("fragment")]
float4 contactShadowFragment(AoOut input) : SV_Target
{
    Frame* frame = push.frame;
    uint depthTexture = push.textures.x;
    int2 pixel = int2(input.position.xy);
    float depth = loadDepth(depthTexture, pixel);
    if (depth <= 0.0 || dot(frame->sunColor, frame->sunColor) <= 0.0)
    {
        return 1.0;
    }
    float3 centre = viewPosition(float2(pixel) + 0.5, depth);
    float3 normal = viewNormal(depthTexture, pixel, centre);
    // Into view space, and two pixels' width off the surface, so it does not shadow itself.
    float3 toSun = normalize(transform(frame->view, float4(-frame->sunDirection, 0.0)).xyz);
    // Facing away from the sun, it is dark already.
    if (dot(normal, toSun) <= 0.0)
    {
        return 1.0;
    }
    float pixelSpan = abs(frame->post.aoUnproject.y) * 2.0 / float(frame->viewport.y) * centre.z;
    float3 start = centre + normal * (2.0 * pixelSpan);
    uint steps = max(frame->post.contactSteps, 1);
    float stepLength = frame->post.contactLength / float(steps);
    float jitter = aoNoise(pixel, frame->frameIndex).x;
    float2 size = float2(frame->viewport);
    for (uint i = 0; i < steps; i++)
    {
        float3 along = start + toSun * stepLength * (float(i) + jitter);
        // Back onto the screen, as viewPosition came off it.
        float2 ndc = along.xy / (along.z * frame->post.aoUnproject) + frame->jitter * 2.0 / size;
        float2 at = (ndc * 0.5 + 0.5) * size;
        if (along.z <= frame->near || any(at < 0.0) || any(at >= size))
        {
            break;
        }
        float scene = loadDepth(depthTexture, int2(at));
        float sceneZ = frame->near / max(scene, 1e-7);
        float behind = along.z - sceneZ;
        if (behind > 0.0 && behind < frame->post.contactThickness)
        {
            return 0.0;
        }
    }
    return 1.0;
}

// ---------------------------------------------------------------- Screen-space reflections

// Where a point in view space lands on the screen, in pixels, as
// viewPosition came off it.
float2 reflectionPixel(Frame* frame, float3 point, float2 size)
{
    float2 ndc = point.xy / (point.z * frame->post.aoUnproject) + frame->jitter * 2.0 / size;
    return (ndc * 0.5 + 0.5) * size;
}

// The levels of the nearest-depth pyramid screen-space reflections step
// through (src/ambient_occlusion.rs builds it): cells up to 64 pixels across.
static const uint REFLECTION_PYRAMID_LEVELS = 7;

// A world position from a point on the screen, in pixels, at a depth.
float3 worldAtPixel(Frame* frame, float2 pixel, float depth)
{
    float2 uv = pixel / float2(frame->viewport);
    float4 world = transform(frame->invViewProj, float4(uv * 2.0 - 1.0, depth, 1.0));
    return world.xyz / world.w;
}

// Screen-space reflections, at full resolution: from each pixel, its mirror
// ray walked through the nearest-depth pyramid (after Uludag, "Hi-Z
// Screen-Space Cone-Traced Reflections", 2014). On screen, with reverse-Z,
// a ray's depth changes evenly from pixel to pixel, so it can step a whole
// cell of a coarse level at once wherever it passes in front of the nearest
// thing in that cell, and drops to finer levels only where it reaches it,
// down to the pixel it meets. Unlike steps of a set length, it misses
// nothing it passes, and a surface seen edge on, a few pixels thick on
// screen, is met where the ray crosses it.
//
// Where it meets something, the colour comes from last frame's finished
// picture (TAA's history), found where that point was on screen then, as
// this frame's is not shaded yet. Where it finds nothing, or leaves the
// screen, the reflection is what the surroundings show (reflectionFallback):
// the reflection probes round the surface, then the GI's probes as seen from
// as far as the ray got, then the sky. Writes the reflection and how much of
// it the ray found, which fades towards the screen's edges and the ray's
// end. A pixel too rough to show a reflection, as the depth passes wrote its
// roughness, traces nothing. textures: x depth, y last frame's picture, z
// the surface (normal and roughness), w the nearest-depth pyramid.
[shader("fragment")]
float4 reflectionsFragment(AoOut input) : SV_Target
{
    Frame* frame = push.frame;
    uint depthTexture = push.textures.x;
    int2 pixel = fullPixel(int2(input.position.xy));
    float depth = loadDepth(depthTexture, pixel);
    float4 surface = textures[NonUniformResourceIndex(push.textures.z)].Load(int3(pixel, 0));
    float roughness = surface.b;
    if (depth <= 0.0 || roughness >= frame->post.reflectionMaxRoughness)
    {
        return 0.0;
    }
    float3 centre = viewPosition(float2(pixel) + 0.5, depth);
    // The surface's own smooth normal, as the depth passes kept it, not one from the depth: a sphere's depth is
    // its flat triangles, each a flat mirror showing one place, so a chrome ball's reflection broke into blocks a
    // triangle wide; and while two of its LODs crossfade, dithered, the depth's neighbours are on either one, and
    // its reflection broke into speckles.
    float3 worldNormal = octDecode(surface.rg * 2.0 - 1.0);
    float3 normal = normalize(transform(frame->view, float4(worldNormal, 0.0)).xyz);
    float facing = dot(normal, centre) > 0.0 ? -1.0 : 1.0;
    normal *= facing;
    worldNormal *= facing;
    float3 ray = reflect(normalize(centre), normal);
    // The same in the world, for what the ray finds when it finds nothing on screen.
    float3 position = worldAtPixel(frame, float2(pixel) + 0.5, depth);
    float3 worldRay = reflect(normalizeOr(position - frame->cameraPosition, frame->cameraForward), worldNormal);
    bool atmosphere = frame->lighting.sky == SKY_ATMOSPHERE;
    float3 outside = position + worldNormal * ddgiSpacing(frame, 0);
    float3 around = reflectionAmbient(frame, outside, worldNormal, atmosphere);

    // Rays back towards the camera find only what is behind it, off screen. They fade out on the way there: cut off
    // at once, a chrome ball's lower half showed a sharp ring where they gave way to the sky's ground colour.
    float towards = smoothstep(-0.2, 0.0, ray.z);
    float2 size = float2(frame->viewport);
    float pixelSpan = abs(frame->post.aoUnproject.y) * 2.0 / size.y * centre.z;
    float3 start = centre + normal * (2.0 * pixelSpan);
    float reach = frame->post.reflectionDistance;
    // Stopped short of the near plane, so its depth stays finite.
    if (start.z + ray.z * reach < frame->near * 1.5)
    {
        reach = (frame->near * 1.5 - start.z) / ray.z;
    }
    float3 finish = start + ray * max(reach, 0.0);
    // The ray on screen: pixels across and down, and depth, each changing evenly along it.
    float3 from = float3(reflectionPixel(frame, start, size), frame->near / start.z);
    float3 to = float3(reflectionPixel(frame, finish, size), frame->near / finish.z);
    float3 span = to - from;
    // Cut where it leaves the screen.
    float2 leave = select(span.xy > 0.0, (size - 0.5 - from.xy) / span.xy, (0.5 - from.xy) / span.xy);
    leave = select(abs(span.xy) < 1e-4, float2(1.0), leave);
    float last = saturate(min(1.0, min(leave.x, leave.y)));

    uint pyramid = push.textures.w;
    uint iterations = max(frame->post.reflectionSteps, 4);
    int level = 0;
    // A pixel and a half along, off the surface it leaves, and a little more by noise, so neighbouring rays do not
    // band together.
    float along = length(span.xy) > 1e-3 ? (1.5 + aoNoise(pixel, frame->frameIndex).y) / length(span.xy) : 1.0;
    float t = along;
    bool found = false;
    for (uint i = 0; i < iterations && t < last; i++)
    {
        float3 at = from + span * t;
        float cellSize = float(1 << level);
        float2 cell = floor(at.xy / cellSize);
        // Where the ray leaves this cell, a hair past its side.
        float2 side = (cell + select(span.xy > 0.0, float2(1.0), float2(0.0))) * cellSize;
        float2 crossing = select(abs(span.xy) < 1e-6, float2(1e9), (side - from.xy) / span.xy);
        float leaves = min(crossing.x, crossing.y) + 0.01 / max(length(span.xy), 1e-3);
        float nearest = textures[NonUniformResourceIndex(pyramid)].Load(int3(int2(cell), level)).r;
        if (at.z > nearest)
        {
            // In front of everything in the cell: on to its side, or, where the ray reaches the nearest depth inside
            // it, down a level from there.
            float reaches = span.z < 0.0 ? (nearest - from.z) / span.z : 1e9;
            if (reaches >= leaves)
            {
                t = leaves;
                level = min(level + 1, int(REFLECTION_PYRAMID_LEVELS) - 1);
                continue;
            }
            t = max(t, reaches);
            // Within a single pixel, where it reaches the depth is where it meets what is there.
            if (level == 0)
            {
                found = true;
                break;
            }
            level--;
            continue;
        }
        if (level > 0)
        {
            level--;
            continue;
        }
        // At or behind the depth of this pixel: met, if not too far behind it; else it passes behind something,
        // and goes on beyond it.
        float sceneZ = frame->near / max(nearest, 1e-7);
        float rayZ = frame->near / max(at.z, 1e-7);
        float thickness = frame->post.reflectionThickness * (1.0 + 0.02 * sceneZ);
        if (rayZ - sceneZ < thickness)
        {
            found = true;
            break;
        }
        t = leaves;
    }
    // Out of cells to visit before the ray's end, as along a floor seen low down, whose nearer rows fill every
    // coarse cell the ray passes over and keep it to the finest levels: the rest of the way in a few long steps,
    // each that crosses behind the depth halved down to where it did.
    if (!found && t < last)
    {
        static const uint LONG_STEPS = 12;
        float stride = (last - t) / float(LONG_STEPS);
        float front = t;
        for (uint i = 1; i <= LONG_STEPS && !found; i++)
        {
            float next = t + stride * float(i);
            float3 at = from + span * next;
            float sceneDepth = loadDepth(depthTexture, int2(at.xy));
            if (at.z > sceneDepth)
            {
                front = next;
                continue;
            }
            float back = next;
            for (uint halving = 0; halving < 6; halving++)
            {
                float middle = (front + back) * 0.5;
                float3 point = from + span * middle;
                bool behind = point.z <= loadDepth(depthTexture, int2(point.xy));
                back = behind ? middle : back;
                front = behind ? front : middle;
            }
            float3 crossing = from + span * back;
            float sceneZ = frame->near / max(loadDepth(depthTexture, int2(crossing.xy)), 1e-7);
            float rayZ = frame->near / max(crossing.z, 1e-7);
            // Looser than a pixel's walk, by how far a long step goes.
            float thickness = frame->post.reflectionThickness * (1.0 + 0.02 * sceneZ) + 0.1 * stride * length(span.xy) * abs(frame->post.aoUnproject.y) * 2.0 / size.y * sceneZ;
            if (abs(rayZ - sceneZ) < thickness)
            {
                found = true;
                t = back;
            }
            else
            {
                front = next;
            }
        }
        t = found ? t : last;
    }
    t = min(t, last);
    float3 ended = from + span * t;

    // How far along the ray it got, in the world: the free stretch the GI's probes are looked up at the end of.
    float3 endView = viewPosition(ended.xy, ended.z);
    float travelled = length(endView - start);
    float3 end = position + worldRay * max(travelled - ddgiSpacing(frame, 0) * 0.5, 0.0);
    end = travelled > ddgiSpacing(frame, 0) ? end : outside;
    float3 surroundings = reflectionFallback(frame, position, end, -worldRay, worldRay, roughness, around, atmosphere, true);
    if (!found)
    {
        return float4(surroundings, 0.0);
    }
    // Where the point it met was last frame.
    float2 hit = ended.xy;
    float2 uv = hit / size;
    float hitDepth = loadDepth(depthTexture, int2(hit));
    float4 world = transform(frame->invViewProj, float4(uv * 2.0 - 1.0, hitDepth, 1.0));
    float4 before = transform(frame->prevViewProj, float4(world.xyz / world.w, 1.0));
    float2 previousUv = before.xy / before.w * 0.5 + 0.5;
    if (before.w <= 0.0 || any(previousUv < 0.0) || any(previousUv > 1.0))
    {
        return float4(surroundings, 0.0);
    }
    float3 colour = textures[NonUniformResourceIndex(push.textures.y)].SampleLevel(samplers[SAMPLER_LINEAR_CLAMP], previousUv, 0.0).rgb;
    float2 border = min(uv, 1.0 - uv);
    float sure = saturate(min(border.x, border.y) * 10.0) * saturate((1.0 - travelled / max(reach, 1e-3)) * 4.0) * towards;
    return float4(lerp(surroundings, colour, sure), sure);
}
