---
order: 6
---

A terrain's **surfaces** are what its ground is covered with: dirt, mud, grass, rock, snow. Each surface is a material laid on the ground, its pictures repeated every few metres. A terrain can have up to 32 surfaces. Without any, it is drawn with its **Material** (see [[Terrains]]).

## The surface list

Pick **Paint** along the top of the Terrain in the Inspector. Its surfaces are listed with the top one first, as layers are, and the ground's own surface at the bottom.

- **Add Surface** opens the picker of the project's materials; the one you pick is added on top and picked.
- A click picks a surface: its settings show below the list, and the brush paints it (see [[Painting]]).
- Drag a surface by its grip to move it up or down; its cross takes it out, with what was painted of it.

Each change to the list is one step to undo.

## Where each surface covers

The surfaces stack from the bottom up. The first is the ground under the rest, wherever the others leave it bare. Each one above it covers those below as far as its **Cover** says:

| Cover | Where it covers |
| --- | --- |
| Painted | Only where it is painted |
| Everywhere | All the ground, but where painting takes it away |
| Height and Slope | Where the ground is between the heights and slopes its rules give, and where it is painted |

Rules follow the ground. A hill raised later is snowy at its top and rocky on its sides with no repainting, and a crater dug from a script turns its fresh walls to dirt.

A typical battlefield takes three: mud as the ground's surface, packed dirt **Everywhere** above it with a path painted away through it, and rock by **Height and Slope** on everything steeper than 35°.

## Settings

| Setting | What it does |
| --- | --- |
| Material | The material it is drawn with. Its colour, normal, metal and roughness, and height pictures are used |
| Tile size | How many metres one picture of the material covers on the ground |
| Height blend | How it meets the surfaces beside it, from 0 to 1: blended evenly at 0; towards 1, the higher bumps of its height picture stand through the other's, so stones rise out of mud where they meet |
| Triplanar | Draws it from the side as well as from above, so its pictures do not stretch down cliffs. It costs more, so give it to the surfaces that cover steep ground |
| Tracks | How far feet and wheels press into it, from 0 to 1: 1 for fresh snow, about a half for mud, 0 for rock and road (see [[Tracks and Snow]]) |
| Fills in | With tracks, how many seconds they take to fill in again, as falling snow fills them. 0 keeps them |
| Liquid | None, **Wet**, **Puddles** or **Lava** (see [[Wet Ground, Puddles and Lava]]) |
| Glitter | Sparkles in the sun, as snow and sand do |
| Cover | Where it covers, as above. The ground's own surface has none |

With **Height and Slope**, its rules:

| Setting | What it does |
| --- | --- |
| Height mask | Covers only between two heights |
| Heights | The lowest and highest, in metres |
| Slope mask | Covers only between two slopes |
| Slopes | The least and most steep, in degrees: 0 is level, 90 a wall |
| Feather | How far it eases in at each edge, in metres for heights and degrees for slopes |
| Breakup | How far its edges wander, in metres or degrees, so they do not run straight along the hills' contours |
| Breakup size | How large the wanderings are, in metres |

## How surfaces are drawn

Each point of the ground keeps its four largest surfaces and how much of each, worked out from the covers and what was painted, and kept beside its heights in the `.heights` file. A game reads them as they are; the editor works them out again where the ground or the surfaces change. Between points, the surfaces blend, and then by their height pictures as their **Height blend** asks.

A material with no height picture blends by its share alone. A surface drawn **Triplanar** blends the three directions by which way the ground faces.
