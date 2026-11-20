---
order: 3
---

Every place something can stand is an **area** of ground, and each area costs something to cross. Things finding their way take the cheapest way, not the shortest: a villager keeps to the road through a town, a guard goes round the ford rather than wading it, and nothing walks on lava.

| Area | What it is | Cost |
| --- | --- | --- |
| Walkable | Plain ground | 1 |
| Road | Ground marked as a road | 0.6 |
| Water | Water a block deep, waded | 3 |
| Swim | Water too deep to stand in, swum by what swims | 4 |
| Door | A shut door, opened by what opens doors | 2 |
| Not Walkable | Ground nothing stands on | Never crossed |
| Custom 1 to 4 | Your own, named as you like | 1 |

A cost of 1 is plain ground. Under 1 is sought out: at 0.6 a way along a road goes nearly twice as far to stay on it. Over 1 is kept off where there is another way.

## Setting the costs

Open **Project Settings ▸ Navigation**. **Costs** sets the cost of a road, water, swimming and a door. **Your Own Areas** names your four areas, such as Mud or Danger, and sets what each costs.

## Marking ground

- **By block:** select a `.block` and set its **Nav Area**. Every place standing on that block is of that area: a cobblestone block that is Road, a farmland block that is Not Walkable. **As It Is** leaves it plain ground, water waded or a door, as the block is.
- **By box:** choose **Add Component ▸ Navigation ▸ Nav Area** on an Obj and set its **Size** and **Area**. Every place whose feet are in the box is of that area, whatever it stands on: a road through a town without changing its blocks, or Not Walkable round a campfire.

## From a script

`Navigation.AreaAt(point)` says what area the place to stand nearest a point is, as a `NavAreaType`.

`Obj.NavArea` is an Obj's Nav Area, null when it has none, and `Obj.AddNavArea` gives it one: its `Size`, and its `Mark`, a `NavAreaMark`. Ways searched after a change go by it, as a bridge that burns is kept off:

```csharp
Obj.NavArea?.Mark = NavAreaMark.NotWalkable;
```
