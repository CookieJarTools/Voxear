// Small voxels traced on the GPU, one thread a pixel (common.slang has the world and the walk). With
// FROM_COARSE, a coarse pass first walks one ray every few pixels against whole bricks, and each pixel's ray starts
// from the nearest of the distances its corners reached, less a brick.

#include "common.slang"

[shader("compute")]
[numthreads(8, 8, 1)]
void coarseView(uint3 id: SV_DispatchThreadID)
{
    View* view = push.view;
    Draw* draw = push.draw;
    if (id.x >= draw->coarseSize.x || id.y >= draw->coarseSize.y)
    {
        return;
    }
    float3 d = rayOf(view, float2(id.xy * draw->coarseStep));
    Hit hit;
    // Every brick taken as solid: the walk stops at the first brick with anything in it.
    bool found = trace(push.world, view->eye, d, 1e9, 8.0, 0.0, hit);
    draw->coarse[id.y * draw->coarseSize.x + id.x] = found ? hit.t : 1e9;
}

[shader("compute")]
[numthreads(8, 8, 1)]
void traceView(uint3 id: SV_DispatchThreadID)
{
    View* view = push.view;
    World* world = push.world;
    if (id.x >= view->size.x || id.y >= view->size.y)
    {
        return;
    }
    float3 d = rayOf(view, float2(id.xy) + 0.5);
    float3 eye = view->eye;
    float skipped = 0.0;
    if ((view->flags & FROM_COARSE) != 0u)
    {
        Draw* draw = push.draw;
        uint2 c = id.xy / draw->coarseStep;
        uint w = draw->coarseSize.x;
        float* t = draw->coarse;
        float nearest = min(min(t[c.y * w + c.x], t[c.y * w + c.x + 1]), min(t[(c.y + 1) * w + c.x], t[(c.y + 1) * w + c.x + 1]));
        skipped = max(nearest - 12.0, 0.0);
        if (nearest >= 1e9)
        {
            view->output[id.y * view->size.x + id.x] = finish(skyOf(d), view->flags, 0);
            return;
        }
        eye += d * skipped;
    }
    float footScale = view->pixelAngle * view->lodScale;
    Hit hit;
    bool found = trace(world, eye, d, 1e9, skipped * footScale, footScale, hit);
    int steps = hit.steps;
    float3 colour;
    if (found)
    {
        hit.t += skipped;
        colour = shadeHit(world, view, d, hit, steps);
    }
    else
    {
        colour = skyOf(d);
    }
    view->output[id.y * view->size.x + id.x] = finish(colour, view->flags, steps);
}
