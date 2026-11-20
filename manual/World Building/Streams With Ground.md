---
order: 3
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

## Which ground is made

`vox editor call blocks.columns` lists each world's columns made last frame, those that came and went, and how many Objs are kept while their ground is away.
