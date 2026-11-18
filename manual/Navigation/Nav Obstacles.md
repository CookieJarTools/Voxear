---
order: 6
---

A **Nav Obstacle** is a box that things finding their way go round, though it was not there when the nav was made: a crate pushed into a doorway, a cart left in a street, a gate swung shut, a barricade the players build. Walls and blocks are already in the nav. A Nav Obstacle is for what moves, or comes and goes, while the game runs.

## Shutting a doorway with a crate

1. Select the crate.
2. Choose **Add Component ▸ Navigation ▸ Nav Obstacle**.
3. Set **Size** to the crate's size. The box is centred on the Obj and turns and scales with it.

Play, and agents walking to the doorway go another way, or stop at the nearest place they can reach when the doorway was the only way. Push the crate out of the doorway and, once it stands still, the way is open again.

## Carving and going round

Standing still, an obstacle **carves**: it is cut out of the nav, so ways are found round it before anything sets off. It carves at once when the game starts, and again whenever it has stood still for **Still Time**.

Moved further than **Move Threshold**, its cut is let go. While it moves, agents near it turn aside as they do for each other, as for a cart rolling down a street. Turn **Carve** off for something that is always on the move: it is then only ever gone round.

## Settings

| Setting | What it does |
| --- | --- |
| Size | How wide, high and long the box is, in metres, centred on the Obj and turned and scaled with it |
| Carve | On, it is cut out of the nav where it stands still. Off, it is only gone round |
| Move Threshold | How far it moves, in metres, before it counts as moving and its cut is let go. Default 0.1 |
| Still Time | How long it stands still, in seconds, before it carves where it stopped. Default 0.5 |

A box carves the floors it stands in the way of: a floor under it by less than an agent is tall, up to its top. A beam high over heads, or a crate on the floor above, leaves the floor below open.

## In a Block World and on a level of models

In a [[Block Worlds|Block World]] the places under a carved obstacle are Not Walkable, as under a [[Areas and Costs|Nav Area]] of that kind, from the next step. Dig or place blocks and nothing changes for the obstacle.

On a [[Levels of Models|level of models]] the baked nav mesh is cut round each carved obstacle on navigation's own threads, from the mesh as baked each time, so the cut follows the obstacle and nothing piles up. A carving obstacle's own colliders are left out of **Bake Navigation**: the crate may be pushed somewhere else, and should leave no hole where it stood.

## From a script

```csharp
// A portcullis that shuts the castle to everything finding its way in, and opens it again.
var gate = Obj.AddNavObstacle();
gate.Size = new Vector3(4f, 3f, 0.5f);
gate.Carve = shut;
```

`Obj.NavObstacle` gives an Obj's obstacle. `IsCarving` says whether it is cut out of the nav now.
