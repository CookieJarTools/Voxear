---
order: 4
---

Towns are built from kits of prefabs: houses lined up along a street, each facing it; trees and barrels scattered over a yard; a market's stalls placed one by one. Every prefab placed is a real Obj you can move or delete later, and with a [[Ground Fit]] the ground under it comes back as it was.

## Placement: what a prefab takes up

A **Placement** on a prefab's root says how much ground it takes up (**Size** across and along its front, and its **Centre**), which way its **Front** faces, and where its **Door** is. Lined up along a road, a prefab turns its front to the road. Nothing is placed where a Placement or a Ground Fit already stands. A prefab without a Placement takes up 6 metres square, facing +z.

## The Prefab Brush

1. Make an empty Obj for what you paint, say "Trees", and choose **Add Component ▸ Terrain ▸ Prefab Brush**.
2. Pick prefabs in **Prefab 1** to **Prefab 6**; each shows its picture.
3. With the Obj selected, drag over the ground in the Scene view. Prefabs are scattered within **Radius**, about **Density** each 100 square metres, as real Objs under the brush's Obj. Shift-drag takes them away. Each stroke is one step to undo.

| Setting | What it does |
| --- | --- |
| Mode | **Paint** scatters where a stroke goes. **Single** places one where you click; R turns it by **Turn Step** |
| Radius, Density | How wide the brush is, in metres, and how many it scatters each 100 square metres |
| Random Turn, Turn | Each turned any way, or as **Turn** says |
| Scale | The least and most each is scaled by |
| Lean With Ground | Each leans with the slope under it |
| Steepest | Nothing is put on ground steeper than this |
| Face Road, Setback | In Single, the prefab turns to face the road nearest and sits **Setback** metres back from its edge |

Weights for each prefab, and more than six, come from scripts and the scene file.

## Lining up along a road and scattering over an area

From the command line, with a [[Roads|Road]] or an Area Fill to work from:

```
vox editor call place.line_up road=713a4d989c654c6ca23b1d63201e0090:9 prefabs='[{"prefab":{"path":"Kits/House A.prefab"},"weight":3},{"prefab":{"path":"Kits/Shop.prefab"}}]' side=both setback=1.5 seed=7
vox editor call place.scatter area=713a4d989c654c6ca23b1d63201e0090:12 prefabs='[{"prefab":{"path":"Plants/Oak.prefab"}}]' density=2
vox editor call place.prefab prefabs='[{"prefab":{"path":"Kits/Well.prefab"}}]' at=[120,0,96] face_road=true
```

Lined up, each prefab faces the road and stands a little more or less than **setback** back from its edge, no two alike side by side where there are others to choose, skipping places already taken. The same **seed** gives the same street, so a street you do not like is laid again with another. Each call is one step to undo. `place.brush` is a stroke of the Prefab Brush, with `remove` to take away.

## From scripts

`Placing.LineUp` and `Placing.Scatter` make the same streets and scatterings from a script, and return what they made. `Obj.Placement` and `Obj.AddPlacement` give a prefab its Placement, and `Obj.PrefabBrush` and `Obj.AddPrefabBrush` set up a brush.

```csharp
var houses = Placing.LineUp(MainStreet, [new PlacingChoice(House, 3), new PlacingChoice(Shop)], new LineUpSettings(Setback: 1.5f, Seed: 7));
var trees = Placing.Scatter(Park, [new PlacingChoice(Oak)], density: 2f);
```
