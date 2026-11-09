---
order: 7
---

A terrain's **grass** is what grows thickly all over it: grass, weeds, flowers, pebbles, millions of them. There are far too many to place one by one or to keep a list of, so each kind of grass only says how thickly it grows where, and the GPU grows the blades round the camera each frame: the same blade in the same place every time you come back, sized and turned its own way, leaning with the ground. Grass far off thins out to nothing at its draw distance.

Grass is for small things that nothing collides with. Trees, bushes and rocks that a character bumps into, or a script cuts down, are a [[Foliage]]'s plants instead (see [[Painting and Cutting Plants]]).

## Adding grass

Pick **Grass** along the top of the Terrain in the Inspector. Its kinds are listed with the last added first. **Add Grass** opens the picker of meshes, the built-in shapes and each part of the project's models; the one you pick grows everywhere at once.

| Setting | What it does |
| --- | --- |
| Mesh | What each blade or tuft is. A clump of blades in one mesh draws faster than many single blades |
| Material | What it is drawn with. A **Foliage** material sways in the wind and lets the sun through its blades; give cards cut from a picture `cutout = 0.5` in their material file |
| Density | How many grow to a square metre where it grows fully |
| Sizes | The smallest and largest, each blade somewhere between |
| Draw distance | How far from the camera it is drawn, in metres. It thins out over the last fifth |
| Grows | **Everywhere**, only where **Painted**, or **On Surfaces**: on the surface picked under **On surface**, as thickly as that surface covers (see [[Surfaces]]) |
| Steepest ground | The steepest slope it grows on, in degrees; it thins out over the last five |
| Springs back | How many seconds it takes to stand up again where it was trodden flat. 0 leaves it flat |

Grass on a surface follows the surface: paint mud over a field and the grass on the field goes with it, and grass by height and slope rules grows wherever those rules say.

## Painting and digging up grass

With **Grass** picked, the brush in the Scene view grows the picked kind where it goes; hold **Shift** to dig it up. Size and strength are the terrain brush's (**[** and **]**, and with **Shift**). Each stroke is one step to undo. Painting is added to where a kind grows by its own rules, or taken from it, so a path dug through a field of grass stays dug when the field's rules change.

## How grass is drawn

Each view scatters each kind on a grid fixed to the world round its camera, a cell for each blade it could have, and keeps a blade where the kind grows thickly enough there, on the ground's height, out of holes, and in view. Blades nearer the camera are drawn with their mesh's finest level of detail, and further off with coarser ones, each twice as far as the last. Grass is drawn after the terrain, sways in a [[Wind]] Zone's wind, and bends away from what has a **Foliage Interaction**, as a Foliage's plants do. It lies flat where a **Track Maker** has trodden, whether or not the ground there takes tracks, and stands up again over its **Springs back** time (see [[Tracks and Snow]]). It casts no shadows.

Blades are not Objs or instances of the scene's, so a field of half a million costs no more of the scene's 65,536 instances than none. What they cost is the GPU's time drawing them: a dense kind of a heavy mesh drawn far is the slow kind. Keep each mesh's triangles few, the draw distance as short as it looks right, and the density as low as covers the ground.

## What grass keeps

Each kind's painting is kept in the terrain's `.layers` file, and how thickly each kind grows at each point is kept in its `.heights` file, so a built game reads it as made.
