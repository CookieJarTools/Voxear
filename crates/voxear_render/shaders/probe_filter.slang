// A reflection probe's mips (src/reflection_probes.rs): its capture's faces
// averaged down, then each of the probe's own mips filtered with GGX for its
// roughness, from mirror-smooth at the first to fully rough at the last
// (probeMipRoughness).

#include "reflection_probes.slang"

// One mip of the capture, each texel the average of the four above it.
// push.texture: the mip above, as storage; push.output: this mip.
[shader("compute")]
[numthreads(8, 8, 1)]
void probeDownsampleMain(uint3 id : SV_DispatchThreadID)
{
    if (any(id.xy >= push.size))
    {
        return;
    }
    RWTexture2DArray<float4> above = storageArraysRgba16f[push.texture];
    uint2 at = id.xy * 2;
    float4 sum = above[uint3(at, id.z)] + above[uint3(at + uint2(1, 0), id.z)] + above[uint3(at + uint2(0, 1), id.z)]
        + above[uint3(at + uint2(1, 1), id.z)];
    storageArraysRgba16f[push.output][id] = sum * 0.25;
}

// The i-th of n points spread evenly over a square (Hammersley).
float2 probeSpread(uint i, uint n)
{
    return float2(float(i) / float(n), float(reversebits(i)) * 2.3283064365386963e-10);
}

// One mip of a probe's cube, filtered for its roughness from the capture,
// as seen straight on (the view along the normal, as split-sum lighting
// has it). Each of its samples is read from the capture's mip as wide as
// the share of the sphere it stands for, so few samples do not show as
// spots. push.texture: the capture, as a texture; push.output: the mip;
// push.level: which; push.list: the probe's place in the array.
[shader("compute")]
[numthreads(8, 8, 1)]
void probeFilterMain(uint3 id : SV_DispatchThreadID)
{
    if (any(id.xy >= push.size))
    {
        return;
    }
    static const uint SAMPLES = 64;
    float3 normal = normalize(probeFaceDirection(id.z, (float2(id.xy) + 0.5) / float2(push.size)));
    TextureCubeArray capture = cubeArrays[push.texture];
    SamplerState clamped = samplers[SAMPLER_LINEAR_CLAMP];
    uint3 written = uint3(id.xy, push.list * 6 + id.z);
    if (push.level == 0)
    {
        storageArraysRgba16f[push.output][written] = float4(capture.SampleLevel(clamped, float4(normal, 0.0), 0.0).rgb, 1.0);
        return;
    }
    float roughness = probeMipRoughness(float(push.level));
    float a = roughness * roughness;
    float3 up = abs(normal.y) < 0.999 ? float3(0.0, 1.0, 0.0) : float3(1.0, 0.0, 0.0);
    float3 tangent = normalize(cross(up, normal));
    float3 bitangent = cross(normal, tangent);
    uint width, height, layers, mips;
    capture.GetDimensions(0, width, height, layers, mips);
    // The share of the sphere one texel of the capture's first mip covers.
    float texel = 4.0 * PI / (6.0 * float(width) * float(width));
    float3 sum = 0.0;
    float weight = 0.0;
    for (uint i = 0; i < SAMPLES; i++)
    {
        // A half vector by GGX's spread round the normal (Walter et al. 2007), and the light it reflects.
        float2 u = probeSpread(i, SAMPLES);
        float phi = 2.0 * PI * u.x;
        float cosTheta = sqrt((1.0 - u.y) / (1.0 + (a * a - 1.0) * u.y));
        float sinTheta = sqrt(1.0 - cosTheta * cosTheta);
        float3 halfway = tangent * (cos(phi) * sinTheta) + bitangent * (sin(phi) * sinTheta) + normal * cosTheta;
        float3 light = 2.0 * dot(normal, halfway) * halfway - normal;
        float nl = dot(normal, light);
        if (nl <= 0.0)
        {
            continue;
        }
        // With the view along the normal, the half vector's chance is D(h) / 4 for the light it reflects.
        float pdf = distributionGgx(cosTheta, a) / 4.0;
        float share = 1.0 / (float(SAMPLES) * pdf + 1e-6);
        float mip = clamp(0.5 * log2(share / texel) + 1.0, 0.0, float(mips - 1));
        sum += capture.SampleLevel(clamped, float4(light, 0.0), mip).rgb * nl;
        weight += nl;
    }
    storageArraysRgba16f[push.output][written] = float4(sum / max(weight, 1e-6), 1.0);
}
