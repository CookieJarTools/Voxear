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

## How a Stream flows

A Stream flows down its Path from the first point to the last, at **Speed** where its bed is level. Downhill it runs faster, as real rivers do, worked out from how steeply it falls and how deep it is, so a mountain brook rushes and a wide river on the plain idles along. In the middle it runs fastest; at its banks, a third as fast.

How a Stream looks follows how fast it runs, and changes smoothly from one to the next along it:

- **Calm water**, at a metre a second or less, lies near flat: dark and glassy, with slow ripples drifting down it and clear reflections of what stands over it. In its shallows the bed shows through, with the sun's light dappled on it. Still shallows stay clear; they do not foam.
- **Faster water** is rougher, and drawn out into streaks along the current, with thin lines of foam riding down it.
- **Rapids**, where it falls more steeply than its **Rapids** setting (a slope: 0.1 is a metre's drop every ten metres), are white water. The surface heaves up into standing waves and churns. Its crests are white froth with broken edges that glints in the sun, its troughs run clear and green, and spray flies over it. The white water runs on past the foot of the rapids and fades back to calm.

## Rocks and banks

Whatever stands in a Stream disturbs it as fast as it runs. In calm water a rock has only a faint line round it. In fast water the water piles up against its upstream side in a bow wave, it has a white collar, and its wake spreads out downstream in a V: rough water between two low ridges, with a trail of white down the middle. Water running fast over a bed just under its surface breaks white there too, so a shelf of stones makes a riffle.

Nothing needs setting up for this: the Stream sees what is in the picture where it is drawn. That has one limit. A rock that is out of view, or hidden behind something nearer, disturbs nothing until it is seen.

## Where a Stream ends

Where a Stream of water runs into the sea or a Pool, it becomes that water over ten metres from where it reaches it: its surface rises and falls with the waves there, its own ripples give way to them, and it fades into the water from its banks in. Running into the sea, it churns the sea white where it pours in, a jet of foam carried out as far as it runs fast.

Where a Stream of water ends in another that runs on past it, as a brook joins a river, it eases into the river over its last few metres, and the river churns white below where they join. Two Streams ending in each other meet head on: each fades into the other, and both churn white where they meet.

| Setting | What it does |
| --- | --- |
| Liquid | Water, or [[Lava]] |
| Depth | How deep it runs, in metres. Deeper water runs faster downhill |
| Speed | How fast it flows where its bed is level, in metres a second |
| Rapids | How steep it runs, as a slope, before it breaks into white water |
| Deep, Scatter, Clarity, Murk | Its water's colours, as [[The Ocean]] describes them |

Scripts asking about the water over a Stream get its surface and the way it flows, so a [[Floating Objects|floating]] Obj is carried along it.
