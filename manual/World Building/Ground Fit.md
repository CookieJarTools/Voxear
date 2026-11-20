---
order: 2
---

A **Ground Fit** levels the ground a world of blocks without end makes under an Obj, and eases it back to the generator's ground round it. Put one on a prefab's root and every house made from the prefab sets itself into the hillside wherever it is placed.

## Levelling a plot

1. Select the house's root Obj, or open its prefab.
2. Choose **Add Component ▸ Terrain ▸ Ground Fit**.
3. Set **Size** to the house's footprint along x and z, in metres.

Drop the house anywhere in the Scene view. It lands on the ground as it is without its own fit, and the plot under it is levelled at its height. With the house selected, the gizmo draws the footprint at the level it is cut to. A fainter ring shows the margin, and the faintest one shows how far the slope out reaches.

## Settings

| Setting | What it does |
| --- | --- |
| Mode | **Foundation** builds the ground up under it. **Flatten** builds it up and clears the hillside from over it. **Cut** only clears. **Leave** changes no ground, and only clears plants |
| Size, Centre | Its footprint's box along x and z, in metres in the Obj's own space, turned and moved with it |
| Level | **Obj**: the ground is levelled at the Obj's own height. **Point**: at the height of **Point** |
| Point | A point in the Obj's own space, such as its door, so a house meets the street at its door. Shown when **Level** is **Point** |
| Sink | How many blocks into the ground it stands: the ground is levelled that many blocks over its height |
| Margin | How far past the footprint the ground is levelled, in blocks |
| Slope Out | Over how many blocks the ground eases back to the generator's, a block up or down for each block across, so it can be walked. 0 for a sheer edge, as a retaining wall |
| Fill, Top | The blocks made ground is built of and the levelled ground is topped with. **None** for the biome's own. A block the world does not hold yet counts as **None** |
| Clear Plants | Generated trees, plants and structures are left out of its reach |
| Priority | Where two fits meet, the higher priority's ground wins |

Grass and other ground cover grow on the levelled top as on any block of its kind, and Clear Plants does not stop them. Give the plot a **Top** without ground cover, such as gravel or flagstones, to keep it bare under a house.

The footprint is levelled to whole blocks. The slope out steps a block at a time, so it is always walkable at a block's step.

A footprint can have more boxes, and an outline of any shape besides, from a script, from `vox editor call` or in the scene file:

```toml
[obj.ground_fit]
mode = "flatten"
boxes = [{ centre = [0.0, 0.0], size = [10.0, 8.0] }, { centre = [6.0, 0.0], size = [2.0, 4.0] }]
outline = [[-6.0, -5.0], [6.0, -5.0], [0.0, 9.0]]
level = "point"
point = [0.0, 0.0, 4.0]
```

## When the ground under a fit changes

The fit stays where it is and levels the new ground. A city stays as it was built, and nothing moves unless you move it.

## From scripts

`Obj.GroundFit` is an Obj's Ground Fit, null when it has none, and `Obj.AddGroundFit` gives it one. Each setting above is there by its name: `Mode` as a `FitMode`, `Level` as a `FitLevel`, `Fill` and `Top` as `Block`s, and `Boxes` and `Outline` as lists. A generator script can place a whole city this way:

```csharp
public Prefab House;
public Block Cobbles;

void PlaceHouse(Vector3 at, float turn)
{
	var house = House.Instantiate(at, Quaternion.CreateFromYawPitchRoll(turn, 0f, 0f));
	var fit = house.AddGroundFit();
	fit.Boxes = [new FitBox(Vector2.Zero, new Vector2(10f, 8f))];
	fit.Top = Cobbles;
	fit.SlopeOut = 3;
}
```

From the command line, `component.add` and `field.set` work as for any component, and `ground_fit.land` sets an Obj down on the ground as dropping it in the Scene view does:

```
vox editor call component.add obj=713a4d989c654c6ca23b1d63201e0090:7 component=ground_fit
vox editor call field.set obj=713a4d989c654c6ca23b1d63201e0090:7 component=ground_fit field=slope_out value=2
vox editor call ground_fit.land obj=713a4d989c654c6ca23b1d63201e0090:7
```

## Generated structures

A structure's **Ground** in its `.structure` file also takes **Cut**: the hillside cleared from over its footprint, with nothing built up under it (see [[Structures#How the ground meets a structure]]).
