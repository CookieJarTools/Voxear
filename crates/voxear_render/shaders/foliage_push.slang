// Pushes through foliage (see src/foliage.rs): each cell of the grid round
// the camera springs back towards no push, then takes the strongest push of
// the pushers over it, outwards from each, so a trail stays behind them.
// A cell keeps how high the pusher that made its push stood, and how far up
// and down from there it reaches, so plants far above or below it, a tree's
// crown over a path or a field under a bridge, are left alone.

#include "common.slang"

// How far above or below a pusher plants still feel it, past its radius, in metres.
static const float PUSH_REACH_UP = 1.5;

[shader("compute")]
[numthreads(64, 1, 1)]
void pushMain(uint3 id : SV_DispatchThreadID)
{
    Frame* frame = push.frame;
    Pushes pushes = frame->pushes;
    uint index = id.x;
    if (index >= PUSH_CELLS * PUSH_CELLS)
    {
        return;
    }
    // The world cell this slot holds: the one in the grid round the camera whose coordinates match it modulo the grid.
    int2 slot = int2(index % PUSH_CELLS, index / PUSH_CELLS);
    int2 cell = pushes.origin + ((slot - pushes.origin) % int(PUSH_CELLS) + int(PUSH_CELLS)) % int(PUSH_CELLS);
    float2 centre = (float2(cell) + 0.5) * pushes.cell;

    float4* cells = buffers()->foliagePushes;
    float4 held = cells[index];
    held.xy *= exp(-frame->deltaTime / max(pushes.spring, 0.05));
    for (uint i = 0; i < pushes.count; i++)
    {
        float4 pusher = pushes.pushers[i];
        float2 away = centre - pusher.xz;
        float distance = length(away);
        if (distance >= pusher.w)
        {
            continue;
        }
        float strength = pushes.strengths[i / 4][i % 4];
        float2 outward = distance > 1e-3 ? away / distance : float2(0.0, 1.0);
        float2 bent = outward * strength * sqrt(1.0 - distance / pusher.w);
        if (dot(bent, bent) > dot(held.xy, held.xy))
        {
            held = float4(bent, pusher.y, pusher.w + PUSH_REACH_UP);
        }
    }
    cells[index] = held;
}
