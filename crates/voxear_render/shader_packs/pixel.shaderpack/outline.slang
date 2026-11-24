// Pixel's outlines, over the finished picture, a pixel wide: a pixel is darkened where what is beside it lies well
// behind it, round the outline of what stands in front, and where a face meets another at an angle below it or to its
// right, as where a box stands on the floor. The line takes the colour beneath it, darkened towards the shade colour.

float edgeAt(float2 uv, float2 step, float distance, float3 normal, out float crease)
{
    float there = sceneDistance(uv + step);
    // What lies behind by more than a share of the distance, the sky too.
    float behind = saturate((there - distance) / max(distance * 0.04, 0.05) - 1.0);
    crease = dot(sceneNormal(uv + step), normal) < 0.8 && abs(there - distance) < distance * 0.1 ? 1.0 : 0.0;
    return behind;
}

float4 run(PassInput input)
{
    Options o = options();
    float4 color = sceneColor(input.uv);
    if (o.outline <= 0.0 || isSky(input.uv))
    {
        return color;
    }
    float2 texel = 1.0 / input.size;
    float distance = sceneDistance(input.uv);
    float3 normal = sceneNormal(input.uv);
    float creaseRight, creaseDown, creaseLeft, creaseUp;
    float edge = edgeAt(input.uv, float2(texel.x, 0.0), distance, normal, creaseRight);
    edge = max(edge, edgeAt(input.uv, float2(-texel.x, 0.0), distance, normal, creaseLeft));
    edge = max(edge, edgeAt(input.uv, float2(0.0, texel.y), distance, normal, creaseDown));
    edge = max(edge, edgeAt(input.uv, float2(0.0, -texel.y), distance, normal, creaseUp));
    // A crease drawn on one side of it only, so it is a single pixel wide; darkest where something stands on what is
    // below it, the face beneath turned up and this one not, as a box's foot on the floor.
    float crease = max(creaseRight, creaseDown);
    float standing = sceneNormal(input.uv + float2(0.0, texel.y)).y > 0.7 && normal.y < 0.5 ? creaseDown : 0.0;
    float line = max(edge, crease * lerp(0.6, 1.0, standing));
    float3 dark = lerp(o.shade * 0.3, color.rgb * 0.2, 0.3);
    return float4(lerp(color.rgb, dark, saturate(line * o.outline)), color.a);
}
