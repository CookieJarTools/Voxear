// Volumetric fog: the height fog as a volume of froxels (volume.slang), lit
// by the sun through its shadows, by every light in the clusters and by the
// sky, so light shafts show through gaps and lamps glow in the mist.
//
// Two passes, each a fullscreen draw over the volume's image:
// - scattering: each froxel's density and the light it scatters towards the
//   camera, at a depth jittered each frame and blended with last frame's
//   volume where that froxel was, so the jitter averages out;
// - integration: from the camera outwards, the light the fog has added and
//   the share it lets through, by Hillaire's energy-conserving step (2015).
// The fog pass (post.slang) then lays it over the scene.

#include "shading.slang"
#include "volume.slang"

// How much of last frame's volume each froxel keeps.
static const float VOLUME_HISTORY = 0.9;

struct VolumeOut
{
    float4 position : SV_Position;
};

[shader("vertex")]
VolumeOut volumeVertex(uint vertexId : SV_VertexID)
{
    VolumeOut o;
    o.position = float4(fullscreenNdc(vertexId), 0.0, 1.0);
    return o;
}

// The froxel and slice a texel of the volume's image is.
void volumeCell(float2 position, out int2 cell, out int slice)
{
    int2 texel = int2(position);
    slice = texel.x / int(VOLUME_WIDTH);
    cell = int2(texel.x % int(VOLUME_WIDTH), texel.y);
}

// The direction the eye looks through a point on the screen.
float3 screenDirection(Frame* frame, float2 uv)
{
    return eyeRay(frame, uv * 2.0 - 1.0).direction;
}

// The point a screen position shows at a view depth. An orthographic camera's ray starts at the near plane.
float3 atViewDepth(Frame* frame, float2 uv, float depth)
{
    EyeRay ray = eyeRay(frame, uv * 2.0 - 1.0);
    float from = isOrthographic(frame) ? frame->near : 0.0;
    return ray.origin + ray.direction * (depth - from) / max(dot(ray.direction, frame->cameraForward), 1e-3);
}

float fogDensityAt(Frame* frame, float height)
{
    float falloff = max(frame->post.fogHeightFalloff, 1e-4);
    return frame->post.fogDensity * exp(-falloff * (height - frame->post.fogBaseHeight));
}

// The light a local light sends through a point in the fog, towards the
// camera along `toCamera`. Shapes count from their middle, which is near
// enough for mist.
float3 lightInFog(Light light, float3 position, float3 toCamera, float g)
{
    float3 offset = light.position - position;
    float distanceSquared = max(dot(offset, offset), 0.01);
    float window = rangeWindow(distanceSquared, light.range + lightExtent(light));
    if (window <= 0.0)
    {
        return 0.0;
    }
    float3 toLight = offset * rsqrt(distanceSquared);
    float facing = 1.0;
    if (light.kind == LIGHT_SPOT)
    {
        facing = smoothstep(light.cosOuter, light.cosInner, dot(-toLight, light.direction));
    }
    else if (light.kind == LIGHT_RECTANGLE || light.kind == LIGHT_DISC)
    {
        facing = saturate(dot(-toLight, light.direction));
    }
    float3 shadow = localShadow(push.frame, light, position, float3(0.0), false);
    shadow *= localGlass(push.frame, light, position, float3(0.0));
    return light.color * facing * shadow * window / distanceSquared * miePhase(g, dot(-toLight, toCamera));
}

// textures: x last frame's scattering.
[shader("fragment")]
float4 volumeScatterFragment(VolumeOut input) : SV_Target
{
    Frame* frame = push.frame;
    int2 cell;
    int slice;
    volumeCell(input.position.xy, cell, slice);
    float2 uv = (float2(cell) + 0.5) / float2(VOLUME_WIDTH, VOLUME_HEIGHT);
    float far = frame->post.fogVolumeFar;

    // Somewhere new in the froxel's depth each frame, so the history fills it in.
    float noise = frac(52.9829189 * frac(dot(float2(cell), float2(0.06711056, 0.00583715))));
    float jitter = frac(noise + float(frame->frameIndex % 1024) * 0.618033989);
    float depth = max(volumeSliceDepth(float(slice) + jitter, far), frame->near);
    float3 position = atViewDepth(frame, uv, depth);
    float3 toCamera = towardsEye(frame, position);
    float density = fogDensityAt(frame, position.y);
    float g = frame->post.fogAnisotropy;

    bool atmosphere = frame->lighting.sky == SKY_ATMOSPHERE;
    float3 sunLight = atmosphere ? sunThroughAtmosphere(frame) : frame->sunColor;
    // Among blocks lit by the light kept for them, as much of the sky's light and the sun as their sky light lets in,
    // and their blocks' light: mist in a cave is dark, and warm round a torch.
    float2 blockLit;
    float4 blockGlow;
    bool amongBlocks = blockLightAround(position, blockLit, blockGlow);
    float3 light = sunLight * sunShadowHard(frame, position, float3(0.0)) * sunCookieAt(frame, position)
        * glassSun(frame, position, float3(0.0))
        * miePhase(g, dot(-frame->sunDirection, -toCamera));
    if (amongBlocks)
    {
        light *= blockSunShare(blockLit.x, sunShadowsReach(frame, position));
    }
    uint cluster = clusterAt(frame, uv * float2(frame->viewport), depth);
    uint clusterBase = cluster * (frame->clusterMaxLights + 1);
    uint clusterLights = buffers()->clusters[clusterBase];
    for (uint i = 0; i < clusterLights; i++)
    {
        light += lightInFog(buffers()->lights[buffers()->clusters[clusterBase + 1 + i]], position, toCamera, g);
    }
    // The sky lights the fog from all round, scattered evenly.
    float3 sky = atmosphere ? atmosphereAmbient(frame, 0.5, sunLight)
        : gradientAmbient(frame, 0.5);
    // With GI, the light round the froxel instead, which walls and bounced light change.
    float3 around = frame->ddgi.enabled != 0 ? ddgiLightAround(frame, position, toCamera, sky) : sky;
    // Under a bake, the light its probes' grid holds there, from every way.
    float3 baked;
    if (probeVolumeLight(frame, position, float3(0.0), baked))
    {
        around = baked + (frame->bake.source == GI_BOTH && frame->ddgi.enabled != 0 ? around : 0.0);
    }
    if (amongBlocks)
    {
        around = sky * blockSkyShare(blockLit.x, blockGlow.w) + blockGlow.rgb * blockGlowShare(blockLit.y);
    }
    light += around / (4.0 * PI);
    float4 current = float4(frame->post.fogColor * light * density, density);

    // Last frame's value where this froxel's middle was then, unless it lay off screen.
    float3 middle = atViewDepth(frame, uv, volumeSliceDepth(float(slice) + 0.5, far));
    float4 clip = transform(frame->prevViewProj, float4(middle, 1.0));
    float2 previousUv = clip.xy / clip.w * 0.5 + 0.5;
    bool seen = frame->post.fogVolumeReset == 0 && clip.w > 0.0 && all(previousUv >= 0.0) && all(previousUv <= 1.0);
    if (!seen)
    {
        return current;
    }
    float4 previous = volumeSample(push.textures.x, previousUv, volumeSliceAt(clipViewDepth(frame, clip), far) - 0.5);
    return lerp(current, previous, VOLUME_HISTORY);
}

// A thread a column of froxels, walking its slices front to back once and
// writing each as it passes: the light scattered towards the camera in front
// of it, and the share of what lies behind that still shows. textures: x
// this frame's scattering; output, the integrated volume.
[shader("compute")]
[numthreads(8, 8, 1)]
void volumeIntegrateMain(uint3 id : SV_DispatchThreadID)
{
    Frame* frame = push.frame;
    int2 cell = int2(id.xy);
    if (any(cell >= int2(VOLUME_WIDTH, VOLUME_HEIGHT)))
    {
        return;
    }
    float2 uv = (float2(cell) + 0.5) / float2(VOLUME_WIDTH, VOLUME_HEIGHT);
    float far = frame->post.fogVolumeFar;
    float3 direction = screenDirection(frame, uv);
    float alongRay = 1.0 / max(dot(direction, frame->cameraForward), 1e-3);

    float3 added = 0.0;
    float transmittance = 1.0;
    for (int i = 0; i < int(VOLUME_SLICES); i++)
    {
        float4 froxel = volumeLoad(push.textures.x, cell, i);
        // An orthographic camera sees nothing nearer than its near plane, where its rays start.
        float front = isOrthographic(frame) ? max(volumeSliceDepth(float(i), far), frame->near) : volumeSliceDepth(float(i), far);
        float span = max(volumeSliceDepth(float(i + 1), far) - front, 0.0) * alongRay;
        float extinction = max(froxel.a, 1e-6);
        float through = exp(-extinction * span);
        // The light scattered over the step, as it dims along it.
        added += transmittance * froxel.rgb * (1.0 - through) / extinction;
        transmittance *= through;
        storageImagesRgba16f[push.output][int2(cell.x + i * int(VOLUME_WIDTH), cell.y)] = float4(added, transmittance);
    }
}
