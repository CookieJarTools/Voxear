---
order: 4
---

An **Erosion** layer wears the ground below it down as weather does. Rain falls on it and runs off, cutting channels where it runs fast and dropping what it washed away where it slows: in fans at the foot of slopes, along valley floors and in the hollows it fills. Ground steeper than loose ground can stand slumps down to its angle, and heaps as scree at the foot.

It suits ground made by noise or a [[Generating Terrain|generator]], which no weather has touched: put it above them.

## Wearing ground down

1. Pick **Layers** along the top of the Terrain in the Inspector.
2. **Add Layer ▸ Erosion**. It wears the ground below it at once.
3. Change its settings, then press **Erode Again** to wear the ground as they now say.

| Setting | What it does |
| --- | --- |
| Rain | How much rain falls each step, in millimetres. More rain runs off in larger streams that cut deeper channels |
| Steps | How long it rains and runs: more steps wear deeper and carry further |
| Wear | How readily running water picks up ground, 0 to 1 |
| Slump angle | The steepest loose ground stands, in degrees. Steeper ground slumps down to it; 89 for none |

The button reads **Erode Again** whenever the ground below the layer, or its own settings, changed since it last wore it. Until you press it, the layer keeps what it wore before, so changes below it show with that wear on top.

Its **Opacity** wears it partly: 0.5 is half as worn. Its **Blend** stays **Add**, as its wear is how much each point was raised or lowered.

## How erosion is worked out

The GPU wears the ground down step by step, in the editor, in a moment or a few seconds for a large terrain. Water flows between neighbouring points through pipes, faster down steeper drops, and carries ground with it as it goes, none lost on the way, so the ground has as much in it after as before. A terrain with more than 2048 points a side is worn on a coarser grid, and what changed is spread over every point.

What it changed is kept in the terrain's `.layers` file, as what the [[Sculpting]] brushes paint is, and its heights in the `.heights` file, so a game reads the ground as made, the same on every machine, and never wears it itself.

Each time the layer wears the ground is one step to undo.
