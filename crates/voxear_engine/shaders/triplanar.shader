// Triplanar: pictures laid on from above, the front and the side by where
// the surface is, rather than by its UVs, so rocks, cliffs and blocked-out
// levels are covered evenly, with no stretching or seams. A second picture
// can cover what faces up, as grass or snow on rock. It takes the place of
// the material's Colour and Colour Map.

struct Triplanar
{
    [Property] Picture picture;
    [Property, Color] float3 tint = float3(1.0, 1.0, 1.0);
    [Property, Range(0.1, 20), Tooltip("How many metres the picture covers")] float size = 2.0;
    [Property, Range(1, 16), Tooltip("How sharply one side's picture turns to the next")] float sharpness = 4.0;
    [Property] Picture top;
    [Property, Color] float3 topTint = float3(1.0, 1.0, 1.0);
    [Property, Range(0, 1), Tooltip("How much of what faces up the top picture covers. 0 for none")] float topCover = 0.0;
    [Property, Range(0.1, 20), Tooltip("How many metres the top picture covers")] float topSize = 2.0;
    [Property, Tooltip("Lays the pictures on from the Obj's place, so they move with it")] bool local = false;
}

float3 laid(Picture picture, float3 p, float3 weights)
{
    return picture.sample(p.zy).rgb * weights.x + picture.sample(p.xz).rgb * weights.y + picture.sample(p.xy).rgb * weights.z;
}

void surface(Triplanar triplanar, SurfaceInput input, inout SurfaceOutput surface)
{
    float3 p = triplanar.local ? input.position - input.objectPosition : input.levelPosition;
    float3 weights = pow(abs(input.normal), triplanar.sharpness);
    weights /= max(weights.x + weights.y + weights.z, 1e-4);
    float3 color = laid(triplanar.picture, p / triplanar.size, weights) * triplanar.tint;
    if (triplanar.topCover > 0.0)
    {
        float up = 1.0 - triplanar.topCover;
        float cover = smoothstep(up - 0.08, up + 0.08, input.normal.y);
        float3 top = laid(triplanar.top, p / triplanar.topSize, weights) * triplanar.topTint;
        color = lerp(color, top, cover);
    }
    surface.color = color;
}
