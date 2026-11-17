---
order: 2
---

**Navigation** answers questions about getting about a level without a [[Nav Agents|Nav Agent]] having to walk it.

```csharp
using System.Numerics;
using Voxear;

namespace Game;

public sealed class Hunter : Behaviour
{
	public Obj prey;

	protected override void Update()
	{
		// Only give chase where the prey can be reached on foot, and not too far round.
		var way = Navigation.FindPath(Obj.Position, prey.Position);
		if (way.Status == NavPathStatus.Complete && way.Length < 40f)
		{
			Obj.NavAgent.Destination = prey.Position;
		}
	}
}
```

| Call | What it gives |
| --- | --- |
| `Navigation.FindPath(from, to, height)` | The way from one place to another for something that tall: its corners, its length, what it costs, and whether it reaches |
| `Navigation.IsReachable(from, to, height)` | Whether there is a way there at all |
| `Navigation.Nearest(point, reach)` | The nearest place to stand, on its floor, or null |
| `Navigation.RandomPlace(near, radius)` | Somewhere that can be walked to within that far on foot: for wandering |

## How ways are found in a Block World

- Something walks a block at a time, forwards, back, sideways or cornerwise, and never cuts the corner of a wall.
- It climbs a step **one block** high, and up slabs and stairs. It drops down up to **three blocks**.
- It needs as many clear blocks over the ground as it is tall.
- It wades water one block deep, though it costs more than dry ground, and goes round deeper water.
- A way is pulled tight into a few corners to walk straight between, across open ground at any angle.
- In a world without end, ways reach as far as the ground has been made. A goal further off gives a `Partial` way, as near as the made ground goes.

The places are worked out from the blocks as they stream in, chunk by chunk, on threads of their own, and again for each chunk whose blocks change. A block placed in a doorway closes it from the next frame.
