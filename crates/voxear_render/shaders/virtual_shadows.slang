// The sun's virtual shadow maps (src/vsm.rs): where a point falls in the sun's space, which page of which level
// holds it, and the sun's shadow read from the pool, hard or soft.

#pragma once

#include "common.slang"

// How wide a page of `level` is, in metres.
float vsmPageMetres(Frame* frame, uint level)
{
    return frame->vsm.finestTexel * float(VSM_PAGE) * float(1u << level);
}

// Where a point is across and up the sun's view.
float2 vsmAcross(Frame* frame, float3 position)
{
    return float2(dot(position, frame->vsm.right), dot(position, frame->vsm.up));
}

// How deep a point lies along the sun's light, as the pool keeps depth.
float vsmDepth(Frame* frame, float3 position)
{
    return (dot(position, frame->vsm.direction) - frame->vsm.depthMin) / frame->vsm.depthRange;
}

// The level, fractional, whose texels are as wide as a pixel of the view is at `position`, with the bias.
float vsmLevelAt(Frame* frame, float3 position)
{
    float pixel = max(distance(position, frame->cameraPosition), frame->near) / frame->lodScale;
    return log2(max(pixel / frame->vsm.finestTexel, 1.0)) + frame->vsm.bias;
}

// The finest level a point asks for: the one whose texels are no wider than its pixel.
uint vsmLevelFor(Frame* frame, float3 position)
{
    return uint(clamp(floor(vsmLevelAt(frame, position)), 0.0, float(VSM_LEVELS - 1)));
}

// Whether a level's window round the camera holds the page at `across`.
bool vsmWithin(Frame* frame, float2 across, uint level)
{
    int2 at = int2(floor(across / vsmPageMetres(frame, level))) - frame->vsm.origins[level].xy;
    return all(at >= 0) && all(at < VSM_WINDOW);
}

// The level whose texels make `metres` about `texels` of them, no finer than `finest`.
uint vsmLevelForWidth(Frame* frame, float metres, float texels, uint finest)
{
    float level = ceil(log2(max(metres / (texels * frame->vsm.finestTexel), 1.0)));
    return uint(clamp(max(level, float(finest)), 0.0, float(VSM_LEVELS - 1)));
}

// The page table's word for a page of a level: its slot plus one in the low half, and in the high half its slot plus
// one in the moving pool, where what moves stands in it this frame; 0 where none is drawn.
uint vsmEntry(Frame* frame, uint level, int2 page)
{
    int2 at = page - frame->vsm.origins[level].xy;
    if (any(at < 0) || any(at >= VSM_WINDOW))
    {
        return 0;
    }
    return frame->vsm.table[level * uint(VSM_WINDOW * VSM_WINDOW) + uint(at.y * VSM_WINDOW + at.x)];
}

// A point of the sun's space in the pool: in the page of `level` holding it, or of the next coarser level drawn
// there. Its texel's width, in metres, and its level.
struct VsmTap
{
    bool found;
    float2 uv;
    uint level;
    float texel;
    // The page, its slot's first texel in its pool, its width in metres, and the pool: its texture and size.
    int2 page;
    float2 corner;
    float metres;
    uint texture;
    float poolSize;
};

// Where `across` falls in a page's slot, as uv over the pool, kept within the slot, whose border reaches as far as
// any filter centred in the page.
float2 vsmUvIn(VsmTap tap, float2 across)
{
    float border = tap.texel * float(VSM_BORDER);
    float2 within = (across - (float2(tap.page) * tap.metres - border)) / (tap.metres + 2.0 * border) * float(VSM_SLOT);
    return (tap.corner + clamp(within, 1.0, float(VSM_SLOT) - 1.0)) / tap.poolSize;
}

VsmTap vsmTap(Frame* frame, float2 across, uint level)
{
    VsmTap tap;
    tap.found = false;
    tap.uv = 0.0;
    tap.level = level;
    tap.texel = 0.0;
    tap.page = 0;
    tap.corner = 0.0;
    tap.metres = 0.0;
    tap.texture = frame->vsm.pool;
    tap.poolSize = float(VSM_POOL);
    for (uint k = level; k < VSM_LEVELS; k++)
    {
        float metres = vsmPageMetres(frame, k);
        int2 page = int2(floor(across / metres));
        uint entry = vsmEntry(frame, k, page);
        if (entry == 0)
        {
            continue;
        }
        // Where something moves in it this frame, the copy with it drawn over.
        uint moving = entry >> 16;
        bool copied = moving != 0 && frame->vsm.movingPool != NO_TEXTURE;
        uint slot = copied ? moving - 1 : (entry & 0xFFFF) - 1;
        uint slots = copied ? VSM_MOVING_POOL_SLOTS : VSM_POOL_SLOTS;
        tap.texture = copied ? frame->vsm.movingPool : frame->vsm.pool;
        tap.poolSize = float(copied ? VSM_MOVING_POOL : VSM_POOL);
        // The slot holds the page with VSM_BORDER texels of border all round.
        tap.found = true;
        tap.level = k;
        tap.texel = metres / float(VSM_PAGE);
        tap.page = page;
        tap.corner = float2(slot % slots, slot / slots) * float(VSM_SLOT);
        tap.metres = metres;
        tap.uv = vsmUvIn(tap, across);
        return tap;
    }
    return tap;
}

// How much of the sun a tap at `across`, within VSM_BORDER texels of `centre`'s page, lets through at `depth`: read
// from the centre's own slot, whose border holds it.
float vsmLit(VsmTap centre, float2 across, float depth)
{
    Texture2D pool = textures[NonUniformResourceIndex(centre.texture)];
    return pool.SampleCmpLevelZero(comparisonSamplers[COMPARISON_SAMPLER_SHADOW], vsmUvIn(centre, across), depth);
}

// The sun's shadow with a hard edge: 3x3 taps of hardware 2x2 filtering at the finest level drawn round the point,
// which is pushed off its surface by a texel and a half of it against acne. Lit beyond every page.
float vsmShadowHard(Frame* frame, float3 position, float3 normal)
{
    VsmTap centre = vsmTap(frame, vsmAcross(frame, position), vsmLevelFor(frame, position));
    if (!centre.found)
    {
        return 1.0;
    }
    float3 offset = position + normal * centre.texel * 1.5;
    float depth = vsmDepth(frame, offset);
    float2 across = vsmAcross(frame, offset);
    float lit = 0.0;
    for (int y = -1; y <= 1; y++)
    {
        for (int x = -1; x <= 1; x++)
        {
            lit += vsmLit(centre, across + float2(x, y) * centre.texel, depth);
        }
    }
    return lit / 9.0;
}

// The finest level a soft shadow's taps read: one whose texels spread its widest reach over no more than this many,
// short of the border, which a tap's 2x2 filter reaches a texel past.
static const float VSM_SOFT_TEXELS = 7.0;
