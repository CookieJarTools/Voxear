---
order: 8
---

**Displacement** makes a surface really bumpy, not just shaded as if it were. Give a material a **Height Map** and a **Displacement**, and every mesh drawn with it rises and sinks by the map: mud churned into ruts, cobbles, rubble, the lip of a trench. Up close, the bumps change the surface's outline against the sky and cast their own shadows.

## Setting it up

1. Select a material and pick a picture for its **Height Map**. White is high and black is low.
2. Set **Displacement**: how far apart, in metres, the map's lowest and highest points are. Half of it rises above the mesh and half sinks below, so the mesh stays in the middle.
3. Set **Detail** if you need to: how many pixels each tiny triangle spans on screen. 6 is the usual; less is finer and costs more.

The two rows show once the material has a height map. A Displacement of 0 turns the effect off.

## How it works

The surface is cut into small triangles on the graphics card every frame, finely where it's near the camera and coarsely far off, so you pay for detail only where it shows. Each edge is cut as finely as its own length on screen asks, and the triangles either side of an edge cut it the same way, so no cracks open between them.

Any mesh works, from a single big quad for a field to a detailed model. Far-off bumps flatten into the map's broad shapes, so the distance doesn't shimmer.

## Things to know

- **UV seams** can open small cracks where the two sides of a seam read different parts of the height map. Keep seams where the map is the same on both sides, or flat.
- Displaced materials can't also be glass, foliage, cut out, or use a custom shader. Those draw as usual, without displacement.
- Bounced light and ray-traced reflections see the flat mesh, not the bumps.
- Lamps' shadows come from the flat mesh. The sun's shadows come from the bumps, for the ground the camera can see.
- Colliders stay the shape of the mesh, so players walk on the flat surface. Keep Displacement modest where they walk.
