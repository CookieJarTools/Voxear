// The evening's light over the scene: a golden haze in the air, thicker far off and towards the sun, and the rays
// in the sun's warm colour. In a cave or under cover, by the sky light round the camera, there is less of both.

float4 run(PassInput input)
{
    Options o = options();
    PackFrame* f = frame();
    float3 color = sceneColor(input.uv).rgb;
    float3 warm = sunLight() * float3(1.0, 0.7, 0.42);
    float3 way = viewRay(input.uv);
    float toward = saturate(dot(way, f->sunDirection));
    // Under the open sky, as the light kept round the camera says; a world without it is all open.
    float open = hasBlockLight() ? smoothstep(0.2, 0.9, lightAround(f->cameraPosition).x) : 1.0;

    // The sky as if a little way off: it is bright already, and more haze there darkens all else as the eye adapts.
    float distance = isSky(input.uv) ? 400.0 : min(sceneDistance(input.uv), 800.0);
    float air = 1.0 - exp(-distance / 500.0);
    float scatter = 0.35 + 1.65 * pow(toward, 6.0);
    float haze = air * scatter * 0.045 * o.haze * open * (1.0 - f->night * 0.8);
    color = lerp(color, warm * 0.5, saturate(haze)) + warm * haze * 0.2;

    if (o.rays)
    {
        float shafts = max(rays.sample(input.uv).r, 0.0);
        color += warm * shafts * 0.25 * o.ray_strength * open;
    }
    return float4(color, 1.0);
}
