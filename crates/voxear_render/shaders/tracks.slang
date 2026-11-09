// Tracks in the ground (see src/tracks.rs): each texel of the map round the camera forgets what it held when it
// has only just come into the map, then each Track Maker that touched the ground since last frame presses its path
// into it, a groove as wide as it is, rounded at its edges. A texel keeps the deepest press and the time of the
// latest, from which the surfaces it is on fill it in again.

#include "common.slang"

// How far above the ground a Track Maker still presses, in metres, the press fading out to there.
static const float TRACK_TOUCH = 0.05;

[shader("compute")]
[numthreads(64, 1, 1)]
void tracksMain(uint3 id : SV_DispatchThreadID)
{
    Frame* frame = push.frame;
    uint index = id.x;
    if (index >= TRACK_TEXELS * TRACK_TEXELS)
    {
        return;
    }
    // The world cell this texel holds: the one in the map whose coordinates match it modulo the map.
    int n = int(TRACK_TEXELS);
    int2 slot = int2(index % TRACK_TEXELS, index / TRACK_TEXELS);
    int2 cell = frame->tracks.origin + ((slot - frame->tracks.origin) % n + n) % n;
    float2 centre = (float2(cell) + 0.5) * frame->tracks.texel;

    float2* texels = buffers()->tracks;
    float2 held = texels[index];
    // One that was not in the map last frame held another cell.
    int2 before = cell - frame->tracks.lastOrigin;
    if (frame->tracks.fresh != 0 || any(before < 0) || any(before >= n))
    {
        held = float2(0.0);
    }

    for (uint i = 0; i < frame->tracks.count; i++)
    {
        float4 path = frame->tracks.paths[i];
        float4 size = frame->tracks.sizes[i];
        // How far the texel is from the path, and how far along it.
        float2 along = path.zw - path.xy;
        float length2 = dot(along, along);
        float t = length2 > 1e-8 ? saturate(dot(centre - path.xy, along) / length2) : 0.0;
        float off = distance(centre, path.xy + along * t) / max(size.x, 1e-3);
        if (off >= 1.0)
        {
            continue;
        }
        float above = lerp(size.z, size.w, t);
        float touch = 1.0 - saturate(above / TRACK_TOUCH);
        float press = size.y * touch * (1.0 - smoothstep(0.55, 1.0, off));
        if (press > 1e-4)
        {
            held = float2(max(held.x, press), frame->time);
        }
    }
    texels[index] = held;
}
