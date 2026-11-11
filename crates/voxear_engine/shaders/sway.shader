// Sway: moves vertices to and fro, more the farther they are from the Obj's
// foot, for flags, washing lines, hanging signs, buoys and plants. The
// motion runs along the shape as a wave, so cloth ripples rather than
// swinging stiffly, and each Obj sways in its own time.

[MaxOffset(2.1)]
struct Sway
{
    [Property, Range(0, 2), Tooltip("How far it sways at most, in metres")] float amount = 0.15;
    [Property, Range(0, 10), Tooltip("How many times a second it sways")] float speed = 0.8;
    [Property, Range(0.1, 20), Tooltip("How far along it from one ripple to the next, in metres")] float rippleLength = 3.0;
    [Property, Tooltip("From the Obj's place to its part that sways most, in metres. Up for a plant or a sign, along the pole for a flag")] float3 reach = float3(0.0, 2.0, 0.0);
    [Property, Tooltip("Which way it sways")] float3 direction = float3(1.0, 0.0, 0.0);
}

float3 vertex(Sway sway, VertexInput input)
{
    float3 along = input.position - input.objectPosition;
    float reach = max(length(sway.reach), 1e-3);
    float3 tip = sway.reach / reach;
    // Still at the foot, swaying most at the far end.
    float far = saturate(dot(along, tip) / reach);
    // Each Obj its own phase, by where it stands from the level's middle.
    float3 placed = input.levelPosition - along;
    float own = frac(sin(dot(placed, float3(12.9898, 78.233, 37.719))) * 43758.5453) * 2.0 * PI;
    float phase = 2.0 * PI * (sway.speed * input.time - dot(along, tip) / sway.rippleLength) + own;
    float3 way = normalize(sway.direction + float3(1e-5, 0.0, 0.0));
    float3 across = cross(way, tip);
    across = length(across) > 1e-3 ? normalize(across) : float3(0.0);
    return (way * sin(phase) + across * 0.3 * sin(phase * 1.7 + 1.3)) * sway.amount * far * far;
}
