// Radiant's light over the scene: the air near the camera with its beams, and beyond it a warm haze that thickens with
// distance, most towards the sun, so far hills fade into it. Under cover, by the sky light round the camera, the far
// haze fades away.

float4 run(PassInput input)
{
    Options o = options();
    PackFrame* f = frame();
    float s = o.strength;
    float3 color = sceneColor(input.uv).rgb;
    float4 close = air.sample(input.uv);
    color = color * close.a + close.rgb;

    float3 way = viewRay(input.uv);
    float toward = saturate(dot(way, f->sunDirection));
    float open = hasBlockLight() ? smoothstep(0.2, 0.9, lightAround(f->cameraPosition).x) : 1.0;
    float low = 1.0 - smoothstep(0.08, 0.7, f->sunDirection.y);
    float day = 1.0 - f->night * 0.8;
    float3 sun = sunLight();
    float3 warm = sun * lerp(float3(1.0, 0.9, 0.8), float3(1.0, 0.68, 0.45), o.warmth * low);

    // The sky only a little way off: it has the air's glow already from the sky program.
    float distance = isSky(input.uv) ? 300.0 : min(sceneDistance(input.uv), 2000.0);
    float thick = 1.0 - exp(-max(distance - 60.0, 0.0) / 900.0);
    float scatter = 0.4 + 2.6 * pow(toward, 5.0) * low;
    float haze = saturate(thick * scatter * 0.09 * o.haze * open * day * s);
    float3 hazeLight = warm * 0.3 + skyAround() * 0.3;
    color = lerp(color, hazeLight, haze);
    return float4(color, 1.0);
}
