---
order: 20
---

A **Block Mob** walks an Obj through a [[Block Worlds|Block World]] by itself. Tell it where to go, or leave it to wander, and it finds its own way over the blocks: round walls, up steps one block high, and down drops. It is what makes an animal, a villager or a monster move about.

A Block Mob only moves the Obj. To see a creature walk, the Obj also needs a model and something to animate it: a [[Box Models|box model]] and a [[Mob Motion]] are made for this.

## Making a creature that wanders

1. Drag a box model into a level that has a Block World, and put it on the ground.
2. With the Obj selected, choose **Add Component ▸ Terrain ▸ Block Mob**. The Obj is given a **Character Controller** as well, which is what holds it on the blocks. Set the controller's **Height** and **Radius** to fit the model.
3. Choose **Add Component ▸ Animation ▸ Mob Motion**, so its legs step as it goes.
4. Set **Wander** on the Block Mob to how far it may stray, such as 8.

Press Play. The creature walks to somewhere nearby, stands a while, and goes somewhere else. Drag the Obj into the Project panel to make a prefab of it, and place or spawn as many as you like.

## Settings

| Setting | What it does |
| --- | --- |
| Speed | How fast it walks, in metres a second |
| Turn Speed | How fast it turns to face the way it goes, in degrees a second |
| Swims | On, it swims up in water too deep to stand in. Off, it sinks and walks the bottom |
| Wander | How far from where it starts it wanders by itself, in metres. 0 stands still until a script tells it where to go |

## How a Block Mob finds its way

- It walks a block at a time, forwards, back or sideways, never through a corner.
- It climbs a step **one block** high. Anything higher is a wall, and it goes round.
- It drops down up to **three blocks**.
- It needs as many clear blocks over the ground as its Character Controller is high.
- It wades water one block deep and goes round deeper water. If it ends up in deep water anyway, it swims.
- It looks for a new way about once a second, so it follows something that moves and notices blocks that were placed or dug.

A place it cannot reach is not an error. It stands, and looks again.

## Telling a Block Mob where to go from a script

```csharp
using System.Numerics;
using Voxear;

namespace Game;

public sealed class Pet : Behaviour
{
	public Obj owner;

	protected override void Update()
	{
		var mob = Obj.BlockMob;
		var far = Vector3.Distance(owner.Position, Obj.Position) > 6f;
		if (far)
		{
			mob.Follow(owner, 2f);
		}
		else if (mob.Arrived)
		{
			mob.Wander(4f);
		}

		Obj.MobMotion.LookAt(owner.Position + Vector3.UnitY * 1.6f);
	}
}
```

| Call | What it does |
| --- | --- |
| `Obj.AddBlockMob()` | Gives the Obj a Block Mob, and a Character Controller when it has none |
| `MoveTo(point)` | Walks to a point in the world |
| `Follow(obj, near)` | Walks after an Obj wherever it goes, and stops within `near` metres of it |
| `Wander(radius)` | Goes here and there, no further than `radius` metres from where it is now |
| `Stop()` | Stands where it is |
| `Arrived` | True when it is where it was sent, or has nowhere to go |
| `Goal` | What it is doing: `Stand`, `MoveTo`, `Follow` or `Wander` |
| `Speed`, `TurnSpeed`, `Swims` | Its settings |

[[Mob Motion]] has its own calls for what a Block Mob cannot know: `LookAt(point)` and `LookAhead()`, `Attack()` and `Hurt()`.

## Not done yet

- Health, damage, drops, spawning and what a creature decides to do. A Block Mob walks where it is told; deciding is a game's own, or a package's.
- Jumping gaps, climbing ladders and opening doors.
- Flying and swimming creatures that find ways through air or water.
- Block Mobs over the network. A game can send where each one is and call `MobMotion.MoveAs` so it walks as it moves.
