---
order: 4
---

A world of blocks without end makes its ground round the player and lets it go behind. **Streams With Ground** makes an Obj go with its ground: a house and the things in it, or the mobs spawned round the player. Without it, an Obj stays where it is when the ground under it goes.

## Setting it up

1. Select the Obj, or the root of its prefab.
2. Choose **Add Component ▸ Terrain ▸ Streams With Ground**.
3. Set **When Unloaded**.

| When Unloaded | What happens when its ground streams out |
| --- | --- |
| Keep For Later | It and everything under it are taken away and kept, and made again exactly as they were when the ground comes back. For houses, props and anything placed in the level |
| Destroy | It is destroyed and does not come back. For mobs a script spawned round the player |
| Stay | Nothing happens. For what must stay, as a quest's target |

An Obj with a [[Ground Fit]] and no Streams With Ground of its own is kept for later. A kept house goes on shaping the ground while it is away, so its plot still shows in the far view and on the map.

It all happens only while the game plays. In the editor nothing goes, so you can build a city far from the camera.

An Obj goes only once the ground under it has been made. One placed or spawned where the ground is not made yet waits for it. The ground it goes with is the 32-block column its Obj stands in.

## From scripts

`Obj.StreamsWithGround` is an Obj's Streams With Ground, null when it has none, and `Obj.AddStreamsWithGround` gives it one:

```csharp
var wolf = WolfPrefab.Instantiate(at, Quaternion.Identity);
wolf.AddStreamsWithGround().WhenUnloaded = WhenUnloaded.Destroy;
```

Behaviours on an Obj that goes are told first, with `OnGroundUnloading`. Write what it must keep into its fields there: an Obj kept for later comes back with its fields as they were, and its behaviours are told with `OnGroundLoaded`.

```csharp
public sealed class Chest : Behaviour
{
	public int Gold;

	protected override void OnGroundUnloading()
	{
		Log.Info($"{Obj} put away with {Gold} gold");
	}
}
```

## Hearing the ground come and go

Something that lives on the ground with no Obj of its own, such as a resident a game keeps as numbers while far off, hears the ground through `Blocks.ColumnMade` and `Blocks.ColumnLetGo`. Each tells of one 32-block column of a world of blocks without end, after the frame it was made or let go in:

```csharp
Blocks.ColumnMade += column =>
{
	foreach (var resident in residents.Where(resident => column.Contains(resident.Home)))
	{
		resident.Wake();
	}
};
```

| On a `BlockColumn` | What it is |
| --- | --- |
| `World` | The world of blocks it is a column of |
| `X`, `Z` | Its place among the world's columns |
| `Size` | How wide it is, in metres |
| `Corner`, `Middle` | Its low corner and its middle along x and z |
| `Contains(position)` | Whether a place stands in it, at any height |

The columns made before the first frame, behind a loading screen, are told of in the first frame. Scripts are told only while the game plays.

To ask instead of waiting, `Blocks.IsMade(position, radius)` says whether the ground within so many metres of a place is made, to see and to stand on. It is true where no world is being made. A column's nav is made a few frames after its blocks: `Navigation.IsReady(position)` says when a Nav Agent can be sent from there.

```csharp
if (Blocks.IsMade(bed) && Navigation.IsReady(bed))
{
	resident.Instantiate(bed);
}
```

## Which ground is made

`vox editor call blocks.columns` lists each world's columns made last frame, those that came and went, and how many Objs are kept while their ground is away.
