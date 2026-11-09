---
order: 2
---

A terrain's ground is made of **layers**, bottom to top, as a picture in a paint program is. Each is a command rather than heights of its own: a hill stamped down, noise over everything, a road laid along a line. Change or move any layer, switch it off or take it out, and the ground is made again from the layers as they are now. Nothing you did before is lost.

## The list

Pick **Layers** along the top of the Terrain in the Inspector. Its layers are listed with the top one first.

- Click a layer to pick it; its settings show under the list.
- Its box switches it on and off.
- Drag it by its grip to move it up or down.
- Its cross takes it out.
- **Add Layer** offers a new layer of each kind, on top.

Each change is one step to undo. A new terrain has no layers: flat ground at its **Base height**.

## Kinds of layer

| Kind | What it makes |
| --- | --- |
| Generate | Ground made by a world generator, from coasts to mountain ranges (see [[Generating Terrain]]) |
| Stamp | A grey picture, or a rounded hill without one, placed, turned and sized on the ground |
| Noise | Rolling, ridged or billowing ground all over, from a seed |
| Sculpt | What the [[Sculpting]] brushes paint |
| Spline | The ground laid level along a line: a road, a riverbed |
| Trench | A trench dug along a line, with its fire step and parapet: a Spline with **Dig** on (see [[Digging]]) |
| Dig | What the **Dig** and **Fill** brushes dug (see [[Digging]]) |
| Terrace | The ground below cut into steps |
| Smooth | The ground below smoothed |
| Import | A grey picture stretched over the whole terrain: heights made in another tool |

A picture is best as a 16-bit greyscale PNG, black lowest and white highest. An 8-bit one has only 256 steps, which show as terraces on a tall terrain. Its top is the far side, the one along +Z, as it looks from above.

## How a layer meets the ground below

| Setting | What it does |
| --- | --- |
| Blend | **Add** raises the ground by the layer's heights, **Subtract** lowers it, **Max** and **Min** keep the higher or the lower, **Set** puts the layer's heights in place, and **Multiply** scales the ground by them |
| Opacity | How much of the layer counts, 0 to 1 |
| Height mask | Keeps the layer to ground between two **Heights**, in metres |
| Slope mask | Keeps the layer to ground between two **Slopes**, in degrees |
| Feather | How far either mask eases in, in metres or degrees |

A stamp of negative **Height** with **Add** digs a pit: a shell crater. A noise layer with a **Slope mask** of 25° to 90° roughens only the steep ground.

## Stamps

| Setting | What it does |
| --- | --- |
| Picture | The grey picture it stamps; None for a rounded hill |
| Position | Where its middle is, in metres from the terrain's corner |
| Rotation | How far it is turned, in degrees |
| Size | How wide and long it is, in metres |
| Height | How high white stands, in metres; below nothing for a pit |
| Falloff | How far in from its edges it eases into the ground, 0 for a hard edge to 1 for all the way |

A picked stamp shows its outline on the ground in the Scene view, with three dots: the one in its middle moves it, the one past its far side turns it, and the one at its corner sizes it. Each drag is one step to undo.

## Splines

A spline layer lays the ground level along its points, **Width** metres across, and eases into the ground beside it over **Falloff** metres. Each point shows as a dot in the Scene view; drag one to move it, and it settles on the ground below the spline there. Its own layer then lays the road along it. A new spline runs across the middle of the terrain.

## Noise, terraces and smoothing

| Kind | Settings |
| --- | --- |
| Noise | **Style** (Fractal, Ridged or Billow), **Scale** (metres to its largest features), **Height**, **Octaves** (how many finer layers of detail), **Seed** |
| Terrace | **Step** (how high each step is, in metres), **Sharpness** (0 for gentle slopes to 1 for cliffs) |
| Smooth | **Radius**, in metres |

The same seed always makes the same ground, on every machine.

## How fast it is

Only the part of the ground a change reaches is made again: a stamp moved makes its old place and its new place again, and the rest is kept. Layers that work on everything, noise and smoothing, make all the ground again, which for a 4097 terrain takes about a third of a second.
