---
order: 3
---

A **Stream** is water running along a **Path**: a river, a brook, or a channel through a town. Add one with **Add Component ▸ Water ▸ Stream**; it brings a Path with it, twenty metres long, bending and falling a little, to shape as you like.

## Drawing the Path

Select the Obj, and its Path shows in the Scene view: a line along its middle, its edges as wide as it is, and arrows pointing the way it flows. Then:

- **Drag a point's dot** to move it. Hold **Ctrl** to snap it to the grid.
- **Drag the small dot at a point's side** to make the Path wider or narrower there.
- **Drag a faint dot halfway along a stretch** to add a point there.
- **Drag the faint dot past the end** to add a point after the last.
- **Shift-click a point** to take it away. A Path keeps at least two.

Each change is one step to undo. The Path runs smoothly through every point, so put the points where the water should go, along the bottom of the valley or channel, a little above its bed. Turn on **Closed** to join the last point back to the first, for a moat or a lazy river.

A Path is a component of its own, under **Add Component ▸ Paths**, so other things can follow one later.

## How it flows

A Stream flows down its Path from the first point to the last, at **Speed** where its bed is level. Downhill it runs faster, as real rivers do, worked out from how steeply it falls and how deep it is, so a mountain brook rushes and a wide river on the plain idles along. In the middle it runs fastest; at its banks, a third as fast.

Where it falls more steeply than **Rapids**, a slope (0.1 is a metre's drop every ten metres), it breaks white, and the white water churns on past the foot of the rapids before it clears.

Its ripples drift down it at its speed, with lines of foam drawn out along the current the faster it runs. Where a Stream runs into the sea or a Pool, it simply runs under their surface.

| Setting | What it does |
| --- | --- |
| Liquid | Water, or [[Lava]] |
| Depth | How deep it runs, in metres. Deeper water runs faster downhill |
| Speed | How fast it flows where its bed is level, in metres a second |
| Rapids | How steep it runs before it breaks white |
| Deep, Scatter, Clarity, Murk | Its water's colours, as [[The Ocean]] describes them |

Scripts asking about the water over a Stream get its surface and the way it flows, so a [[Floating Objects|floating]] Obj is carried along it.
