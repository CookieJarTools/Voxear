---
order: 2
---

# Loading and Memory

Your game only holds in memory what it is using. Loading a level reads what that level needs, and changing level lets go of what the new level does not use. Most games need nothing more than that.

## Loading prefabs

A `Prefab` field loads its prefab along with the Obj it is on, before `Awake`:

```csharp
[SerializeField] private Prefab bullet;
```

To load a prefab only when the game first needs it, use `Ref<Prefab>`, which holds the prefab's path without loading it:

```csharp
[SerializeField] private Ref<Prefab> boss;

private void Arrive()
{
	var loaded = boss.Load();
	loaded.Instantiate(new Vector3(0f, 0f, 20f), Quaternion.Identity);
}
```

Or load one by its path:

```csharp
var crate = Prefab.Load("Prefabs/Crate.prefab");
```

`Load` reads the prefab at once, so a big one can make the game stutter. `LoadAsync` reads it in the background while the game runs:

```csharp
private async void Arrive()
{
	var loaded = await Prefab.LoadAsync("Prefabs/Boss.prefab");
	loaded.Instantiate(new Vector3(0f, 0f, 20f), Quaternion.Identity);
}
```

Loading a prefab that is already loaded costs nothing. If loading fails, the Console says why and you get `Prefab.None`, which `IsValid` reports.

For loading levels, see [[Loading Levels]].

## Letting go

After a level change, Voxear frees everything nothing uses any more: models, materials and prefabs that no Obj in the world draws or came from. You can also ask it to, for example after a big fight:

```csharp
Content.UnloadUnused();
```

A prefab that was freed loads again by itself the next time you use it, so freeing is always safe. To keep one ready, so it never has to load in the middle of play, call `Keep()` on it, and `Release()` when it is no longer needed:

```csharp
private Prefab explosion;

protected override void Start()
{
	explosion = Prefab.Load("Prefabs/Explosion.prefab");
	explosion.Keep();
}
```

## Seeing what is loaded

`Content.Usage` says how many models, prefabs and materials are loaded, and how much the graphics card holds:

```csharp
Log.Info($"Loaded now: {Content.Usage}");
```

The Profiler shows the same under **Frame**. See [[The Profiler]].
