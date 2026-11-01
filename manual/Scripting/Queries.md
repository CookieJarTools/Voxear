---
order: 8
---

A query finds the entities that have certain components, so a system can work through them. Inside a system, write `Query<…>()`; anywhere else, `World.Query<…>()`. A query names up to four components.

## Going through entities

The simplest way is `ForEach`, with `ref` for each component. It takes up to three:

```csharp
Query<Spot, Drift>().ForEach((ref Spot spot, ref Drift drift) =>
{
	spot.Position += drift.Velocity * Time.Delta;
});
```

The fastest way is to go through the query chunk by chunk. A **chunk** is a block of up to a few hundred entities with the same components, kept side by side. Each component is a column you can read or write in one go:

```csharp
foreach (var chunk in Query<Spot, Drift>())
{
	var spots = chunk.Write<Spot>();
	var drifts = chunk.Read<Drift>();
	for (var i = 0; i < chunk.Count; i++)
	{
		spots[i].Position += drifts[i].Velocity * Time.Delta;
	}
}
```

`Read` is for components you only look at, and `Write` for those you change. `chunk.Entities` gives the entities themselves, in the same order. The columns are only good during the loop: do not keep them.

## Narrowing a query

| Add | Keeps only entities that |
| --- | --- |
| `.With<Gone>()` | Also have this component, without reading it |
| `.Without<Gone>()` | Do not have this component |
| `.Changed<Spot>()` | Are in a chunk where Spot was written since this system last ran |

```csharp
foreach (var chunk in Query<Spot>().Without<Gone>())
{
	// …
}
```

`Count()` says how many entities a query finds.

`Changed` works chunk by chunk: writing to one entity counts for its whole chunk. A system's own writes do not count for itself the next time it runs. `ForEach` counts every component it names as written, even ones you only read, so use a chunk loop with `Read` when you filter with `Changed`.

## Adding and destroying while you go through

You can add, remove and destroy while going through a query. Those changes wait until the system's `Update` is over, then apply in order, so the query you are in is not disturbed. Until then, `Has` does not see a component you just added. Changing values happens at once.

```csharp
foreach (var chunk in Query<Spot>().Without<Gone>())
{
	var spots = chunk.Read<Spot>();
	var entities = chunk.Entities;
	for (var i = 0; i < chunk.Count; i++)
	{
		if (spots[i].Position.Y > 100f)
		{
			entities[i].Destroy();
		}
	}
}
```

Next: [[Loading Levels]].
