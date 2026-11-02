---
order: 6
---

Your scripts choose when the game moves to another level. There are two ways.

## Choosing the level in the Inspector

Give your script a `LevelAsset` field, and pick the level in the Inspector, or drag it there from the Project panel:

```csharp
public sealed class Door : Behaviour
{
	public LevelAsset Next;

	public void Open() => Level.LoadAsync(Next);
}
```

The field holds the level by its ID, so renaming or moving the level file keeps it working, where a path typed in a string would break. The examples below write a path, which works too, for code that builds one.

## Loading a level all at once

```csharp
Level.Load("Levels/Night.level");
```

The game finishes the frame, then reads the new level and changes over. It is the simplest way, but the game stops while the level is read, so a big level can make it stutter.

## Loading a level in the background

```csharp
private async void GoToNight()
{
	try
	{
		await Level.LoadAsync("Levels/Night.level");
		Log.Info("Night has come");
	}
	catch (LevelLoadException failure)
	{
		Log.Error(failure.Message);
	}
}
```

`LoadAsync` reads the level while the game keeps running, then changes over at the end of the frame it is ready in. `await` waits for the change, and throws `LevelLoadException` if the level could not load.

While it loads, `Progress` goes from 0 to 1, for a loading bar:

```csharp
var loading = Level.LoadAsync("Levels/Night.level");
// each frame:
Log.Info($"Loading: {loading.Progress:P0}");
```

## Holding the change for a loading screen

To fade out before the new level appears, hold the change until you are ready:

```csharp
var held = LevelLoadOptions.Held;
var loading = Level.LoadAsync("Levels/Night.level", held);
await loading.Ready;      // everything is read in, and waiting
await FadeOut();          // your own fade
loading.Activate();       // let the new level in
await loading;            // and it is there
```

To keep a loading screen up for a moment even when a level loads fast, set `LeastTime`, in seconds:

```csharp
Level.LoadAsync("Levels/Night.level", new() { LeastTime = 1.5f });
```

## Following a change from anywhere

You do not have to keep what `LoadAsync` gives back. The change going on is always `Level.Current`, and `Level` has shortcuts for it, so a loading screen can follow any change, whoever started it:

```csharp
public sealed class LoadingScreen : Behaviour
{
	private UiElement? bar;

	protected override void Start()
	{
		bar = UiDocument.Load("UI/Loading.vui")?.Find("bar");
	}

	protected override void Update()
	{
		bar?.SetStyle($"width: {Level.Progress * 100:0}%");   // full when nothing is loading

		if (Level.IsReady)                                     // a held change is waiting
		{
			Level.Activate();                                  // let it in
		}
	}
}
```

- `Level.Progress` goes from 0 to 1 while a change goes on, and is 1 when none is.
- `Level.IsReady` is true when a held change has read everything and waits.
- `Level.Activate()` lets a held change in. It does nothing when there is none.
- `Level.Next` is the level coming in, and `Level.Error` says why the last change failed.

These read a field, so reading them every frame costs nothing.

## Loading the shared scenes again

Scenes two levels share stay loaded when the game moves from one to the other. To have them load again, so everything in them starts afresh, mark the scene **Reload** in the level (see [[Scenes and Levels]]), or ask for it on one change:

```csharp
Level.LoadAsync("Levels/Night.level", new() { ReloadShared = true });
```

## What stays loaded when the level changes

Scenes the new level shares with the old one stay loaded, untouched, and so do the scenes of a level they both stand on. Only the scenes that differ are unloaded and loaded. Objs marked as persistent stay through every change. See [[Scenes and Levels]].

Objs your scripts make while the game runs belong to the level's active scene, and leave with it.

## Loading single scenes

Levels are the easy way to load scenes. When you want more say, load scenes one at a time with `Scene`. A scene loaded this way joins the scenes loaded now. Nothing else unloads.

```csharp
await Scene.LoadAsync("Scenes/Cellar.scene");
```

As with levels, a `SceneAsset` field lets you pick the scene in the Inspector, and `Scene.LoadAsync`, `Scene.Unload` and `Scene.SetActive` take it as well as a path.

Pass options to change how it comes in:

```csharp
await Scene.LoadAsync("Scenes/Cellar.scene", new() { SetActive = true });   // new Objs go into it, and its sky shows
await Scene.LoadAsync("Scenes/Cellar.scene", new() { Reload = true });      // load it again if it is loaded
```

`Hold = true` makes it wait once everything is read, as a held level change does, until you call `Activate()`.

To take a scene out, or make another the active one:

```csharp
Scene.Unload("Scenes/Cellar.scene");
Scene.SetActive("Scenes/Day.scene");
```

Both happen at the end of the frame. `Scene.ReloadAsync` loads a scene again, so it starts afresh.

`Scene.Loaded` lists every scene loaded now, and `Scene.IsLoaded` says whether one is. They ask the engine each time and make a new list, so read them when you need them rather than every frame.

As with levels, the scene coming in is `Scene.Current`, with `Scene.Progress`, `Scene.IsLoading`, `Scene.IsReady` and `Scene.Activate()` for it.

The level stays the level. When it changes, the scenes the new level does not want unload, the ones you loaded with them. To keep them, change level with `UnloadRest = false`.

## Hearing about level changes

`Level` has events any script can listen to, such as `SwitchStarted` when a change begins and `SwitchFinished` when it is done:

```csharp
protected override void OnEnable()
{
	Level.SwitchFinished += OnLevelChanged;
}

protected override void OnDisable()
{
	Level.SwitchFinished -= OnLevelChanged;
}

private void OnLevelChanged(LevelSwitch change)
{
	Log.Info("A new level is in");
}
```

Always stop listening in `OnDisable`, as above. `Level.Load` does not raise these events: use `LoadAsync` when anything listens.

Only one change happens at a time. Asking for another while one is going gives back the one already going, with a warning.

In the editor, clicking a level in **Levels** while the game plays changes to it this same way, so you can try your loading screens without a menu.
