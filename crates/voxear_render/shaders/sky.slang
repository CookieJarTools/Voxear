// An analytic sky drawn behind everything: only pixels still at the far
// plane (depth 0 with reverse-Z) pass the depth test.

#include "common.slang"

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

[shader("fragment")]
SkyFragmentOut skyFragment(SkyOut input)
{
    Frame* frame = push.frame;
    float4 near = transform(frame->invViewProj, float4(input.ndc, 1.0, 1.0));
    float3 direction = normalize(near.xyz / near.w - frame->cameraPosition);

    float height = direction.y;
    float3 sky = lerp(frame->skyHorizon, frame->skyZenith, pow(saturate(height), 0.45));
    float3 ground = lerp(frame->skyHorizon, frame->groundColor, pow(saturate(-height), 0.35));
    float3 color = height >= 0.0 ? sky : ground;

    float toSun = dot(direction, -frame->sunDirection);
    color += frame->sunColor * (smoothstep(0.99955, 0.9998, toSun) * 8.0 + pow(saturate(toSun), 48.0) * 0.12);
    SkyFragmentOut o;
    o.color = float4(color, 1.0);
    o.velocity = skyMotion(frame, direction);
    return o;
}
