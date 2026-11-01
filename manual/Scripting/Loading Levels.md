---
order: 6
---

Your scripts choose when the game moves to another level. There are two ways.

## All at once

```csharp
Level.Load("Levels/Night.level");
```

The game finishes the frame, then reads the new level and changes over. It is the simplest way, but the game stops while the level is read, so a big level can make it stutter.

## In the background

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
var options = new LevelLoadOptions { LeastTime = 1.5f };
Level.LoadAsync("Levels/Night.level", options);
```

## What stays and what goes

Scenes the new level shares with the old one stay loaded, untouched, and so do the scenes of a level they both stand on. Only the scenes that differ are unloaded and loaded. Objs marked as persistent stay through every change. See [[Scenes and Levels]].

Objs your scripts make while the game runs belong to the level's active scene, and leave with it.

## Hearing about changes

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
