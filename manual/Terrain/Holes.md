---
order: 8
---

A **hole** cuts a terrain's ground away: a way down into a cave, a bunker or a cellar built under it. Nothing is drawn there, nothing stands on it, and rays pass through it.

## Cutting holes

Pick **Sculpt** along the top of the Terrain in the Inspector, then **Holes** as its **Brush**, and paint in the Scene view with the left mouse button. Hold **Shift** as you start a stroke to fill holes instead. The brush's **Size** is its radius; a hole is cut whole, so it has no strength.

Holes need no Sculpt layer: they are the terrain's own, kept in its `.layers` file with what the brushes painted, and in its `.heights` file for a built game. Each stroke is one step to undo.

## How a hole is cut

A hole is cut from the ground's heights: each height is in a hole or not, and every triangle with a corner on a height in a hole is cut away. So a hole's edge steps with the heights, half a metre apart on a terrain of half a metre's resolution. Cover the edge with rocks, a trench's boards or a bunker's frame, as you would in Unity.

What is drawn, what physics collides with and what rays meet are cut the same way, so what you see is what a character falls through. The sun shines through a hole too: the ground round it casts no shadow over it.

## Holes in scripts

In a hole, as off a terrain, there is no ground: **Terrain.TryGetGround** gives false, **Terrain.HeightAt** gives negative infinity, and **Terrain.Raycast** and **Physics.Raycast** pass through to whatever is below.
