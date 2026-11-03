// Ground-truth ambient occlusion (GTAO, Jimenez et al. 2016) as three
// fullscreen passes over the depth the depth passes drew, before the opaque
// passes shade: the occlusion itself, a spatial denoise, and a temporal pass
// that blends it with last frame's. The opaque passes read the result
// (occlusion.slang). The horizon search follows Intel's XeGTAO. See
// src/ambient_occlusion.rs for the order.

#include "common.slang"

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

float loadTexture(uint texture, int2 pixel)
{
    int2 size = int2(push.frame->viewport);
    return textures[NonUniformResourceIndex(texture)].Load(int3(clamp(pixel, int2(0), size - 1), 0)).r;
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
    return viewPosition(float2(pixel) + 0.5, loadTexture(depthTexture, pixel));
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
// textures: x depth. Writes visibility, 1 where nothing occludes.
[shader("fragment")]
float4 gtaoFragment(AoOut input) : SV_Target
{
    Frame* frame = push.frame;
    uint depthTexture = push.textures.x;
    int2 pixel = int2(input.position.xy);
    float depth = loadTexture(depthTexture, pixel);
    if (depth <= 0.0)
    {
        return 1.0;
    }
    float3 centre = viewPosition(float2(pixel) + 0.5, depth);
    float3 normal = viewNormal(depthTexture, pixel, centre);
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

// A 5x5 blur that only mixes pixels lying close to this pixel's plane, so
// occlusion does not bleed across edges onto surfaces in front or behind.
// textures: x GTAO's visibility, y depth.
[shader("fragment")]
float4 aoDenoiseFragment(AoOut input) : SV_Target
{
    uint depthTexture = push.textures.y;
    int2 pixel = int2(input.position.xy);
    float depth = loadTexture(depthTexture, pixel);
    if (depth <= 0.0)
    {
        return 1.0;
    }
    float3 centre = viewPosition(float2(pixel) + 0.5, depth);
    float3 normal = viewNormal(depthTexture, pixel, centre);
    float tolerance = 0.02 * centre.z;
    float sum = 0.0;
    float weight = 0.0;
    for (int y = -2; y <= 2; y++)
    {
        for (int x = -2; x <= 2; x++)
        {
            int2 at = pixel + int2(x, y);
            float distance = abs(dot(viewPositionAt(depthTexture, at) - centre, normal));
            float w = saturate(1.0 - distance / tolerance);
            sum += loadTexture(push.textures.x, at) * w;
            weight += w;
        }
    }
    return sum / max(weight, 1e-4);
}

// ---------------------------------------------------------------- Temporal

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
    float current = loadTexture(push.textures.x, pixel);
    float depth = loadTexture(push.textures.z, pixel);
    if (frame->post.aoReset != 0 || depth <= 0.0)
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
    float low = current;
    float high = current;
    for (int y = -1; y <= 1; y++)
    {
        for (int x = -1; x <= 1; x++)
        {
            float value = loadTexture(push.textures.x, pixel + int2(x, y));
            low = min(low, value);
            high = max(high, value);
        }
    }
    float history = textures[NonUniformResourceIndex(push.textures.y)].SampleLevel(samplers[SAMPLER_LINEAR_CLAMP], previousUv, 0.0).r;
    return lerp(current, clamp(history, low, high), HISTORY);
}
