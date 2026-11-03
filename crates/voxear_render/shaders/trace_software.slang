// Tracing rays in a compute shader, for GPUs without ray tracing: through
// this frame's hierarchy over the scene's instances (built on the CPU each
// frame), then through each instance's mesh's hierarchy over its finest
// LOD's triangles (built when the mesh loads), in the mesh's own space.
// See src/bvh.rs. Included by ddgi.slang with SOFTWARE_RAYS, after `Hit`.

#pragma once

#include "common.slang"

static const uint TRACE_STACK = 32;

// Where a ray enters a box, or beyond `far` when it misses it.
float enterBox(float3 origin, float3 inverse, float3 low, float3 high, float far)
{
    float3 a = (low - origin) * inverse;
    float3 b = (high - origin) * inverse;
    float3 near3 = min(a, b);
    float3 far3 = max(a, b);
    float enter = max(max(near3.x, near3.y), max(near3.z, 0.0));
    float leave = min(min(far3.x, far3.y), min(far3.z, far));
    return enter <= leave ? enter : 1e30;
}

// Pushes a node's two children, `left` and the one after it, those the ray
// enters before `far`, the nearer last so it is walked first: the nearest
// hit is found sooner, and more of the rest is passed over.
void pushNearerLast(BvhNode* nodes, uint base, uint left, float3 origin, float3 inverse, float far,
    inout uint stack[TRACE_STACK], inout uint depth)
{
    BvhNode a = nodes[base + left];
    BvhNode b = nodes[base + left + 1];
    float enterA = enterBox(origin, inverse, a.min, a.max, far);
    float enterB = enterBox(origin, inverse, b.min, b.max, far);
    bool aFirst = enterA <= enterB;
    float nearEnter = aFirst ? enterA : enterB;
    float farEnter = aFirst ? enterB : enterA;
    uint nearChild = aFirst ? left : left + 1;
    uint farChild = aFirst ? left + 1 : left;
    if (farEnter < far && depth < TRACE_STACK)
    {
        stack[depth++] = farChild;
    }
    if (nearEnter < far && depth < TRACE_STACK)
    {
        stack[depth++] = nearChild;
    }
}

// A ray against a triangle, both faces (Möller-Trumbore): how far, and
// where on it, or a miss.
bool hitTriangle(float3 origin, float3 direction, float3 a, float3 b, float3 c, out float t, out float2 where)
{
    float3 ab = b - a;
    float3 ac = c - a;
    float3 p = cross(direction, ac);
    float det = dot(ab, p);
    t = 0.0;
    where = 0.0;
    if (abs(det) < 1e-12)
    {
        return false;
    }
    float inverse = 1.0 / det;
    float3 s = origin - a;
    float u = dot(s, p) * inverse;
    float3 q = cross(s, ab);
    float v = dot(direction, q) * inverse;
    t = dot(ac, q) * inverse;
    where = float2(u, v);
    // A hair beyond the edges, so a ray along the seam where two meshes meet cannot slip between them.
    static const float EDGE = 1e-5;
    return u >= -EDGE && v >= -EDGE && u + v <= 1.0 + EDGE && t > 0.0;
}

// The nearest of a mesh's triangles along a ray in its own space, nearer
// than `nearest` (which it shortens), or any with `any`.
bool traceMesh(uint meshIndex, float3 origin, float3 direction, bool any, inout float nearest, out uint primitive,
    out float2 barycentrics)
{
    primitive = 0;
    barycentrics = 0.0;
    Mesh mesh = buffers()->meshes[meshIndex];
    uint2 start = buffers()->bvhTable[meshIndex];
    float3 inverse = 1.0 / select(abs(direction) < 1e-12, 1e-12, direction);
    uint stack[TRACE_STACK];
    uint depth = 0;
    stack[depth++] = 0;
    bool found = false;
    while (depth > 0)
    {
        BvhNode node = buffers()->bvhNodes[start.x + stack[--depth]];
        if (node.count == 0 && node.leftOrFirst == 0)
        {
            // A mesh with no triangles.
            continue;
        }
        if (enterBox(origin, inverse, node.min, node.max, nearest) >= nearest)
        {
            continue;
        }
        if (node.count == 0)
        {
            pushNearerLast(buffers()->bvhNodes, start.x, node.leftOrFirst, origin, inverse, nearest, stack, depth);
            continue;
        }
        for (uint i = 0; i < node.count; i++)
        {
            uint triangle = buffers()->bvhOrder[start.y + node.leftOrFirst + i];
            uint first = mesh.lods[0].indexOffset + triangle * 3;
            float3 a = buffers()->vertices[mesh.vertexOffset + buffers()->indices[first]].position;
            float3 b = buffers()->vertices[mesh.vertexOffset + buffers()->indices[first + 1]].position;
            float3 c = buffers()->vertices[mesh.vertexOffset + buffers()->indices[first + 2]].position;
            float t;
            float2 where;
            if (hitTriangle(origin, direction, a, b, c, t, where) && t < nearest)
            {
                nearest = t;
                primitive = triangle;
                barycentrics = where;
                found = true;
                if (any)
                {
                    return true;
                }
            }
        }
    }
    return found;
}

// The nearest triangle along a ray, or any at all with `any`, through the
// scene's hierarchy and its instances' meshes'. `far` caps how far.
Hit traceScene(Frame* frame, float3 origin, float3 direction, float far, bool any)
{
    Hit hit;
    hit.found = false;
    hit.t = far;
    hit.instance = 0;
    hit.primitive = 0;
    hit.barycentrics = 0.0;
    float3 inverse = 1.0 / select(abs(direction) < 1e-12, 1e-12, direction);
    uint stack[TRACE_STACK];
    uint depth = 0;
    stack[depth++] = 0;
    while (depth > 0)
    {
        BvhNode node = buffers()->sceneNodes[stack[--depth]];
        if (node.count == 0 && node.leftOrFirst == 0)
        {
            // An empty scene's root.
            continue;
        }
        if (enterBox(origin, inverse, node.min, node.max, hit.t) >= hit.t)
        {
            continue;
        }
        if (node.count == 0)
        {
            pushNearerLast(buffers()->sceneNodes, 0, node.leftOrFirst, origin, inverse, hit.t, stack, depth);
            continue;
        }
        for (uint i = 0; i < node.count; i++)
        {
            TraceInstance instance = buffers()->sceneInstances[node.leftOrFirst + i];
            // Into the mesh's own space, where distances along the ray stay the same since the direction is
            // carried over unnormalized.
            float3 localOrigin = float3(dot(instance.inverse[0], float4(origin, 1.0)),
                dot(instance.inverse[1], float4(origin, 1.0)), dot(instance.inverse[2], float4(origin, 1.0)));
            float3 localDirection = float3(dot(instance.inverse[0].xyz, direction), dot(instance.inverse[1].xyz, direction),
                dot(instance.inverse[2].xyz, direction));
            uint primitive;
            float2 barycentrics;
            if (traceMesh(instance.mesh, localOrigin, localDirection, any, hit.t, primitive, barycentrics))
            {
                hit.found = true;
                hit.instance = instance.instance;
                hit.primitive = primitive;
                hit.barycentrics = barycentrics;
                if (any)
                {
                    return hit;
                }
            }
        }
    }
    return hit;
}
