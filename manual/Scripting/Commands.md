---
order: 5
---

A command is a static method that the editor's command bar (Ctrl+K) can run. Use commands for jobs you do again and again while building levels, such as lining up a row of lamps or renaming a batch of Objs.

```csharp
using Voxear;

namespace MyGame;

internal static class Tools
{
	[Command("Swap Spinners", Keys = "Ctrl+Shift+X")]
	private static void SwapSpinners()
	{
		var a = Obj.Find("Spinner A");
		var b = Obj.Find("Spinner B");
		if (!a.IsValid || !b.IsValid)
		{
			Log.Warning("Spinner A or Spinner B is missing.");
			return;
		}

		(a.Position, b.Position) = (b.Position, a.Position);
	}
}
```

Once your scripts build, **Swap Spinners** is in the command bar, and Ctrl+Shift+X runs it from anywhere in the editor. `Keys` is optional.

- The method must be `static`, take nothing, and return nothing.
- It runs on the level you are editing, and Undo takes back everything it changed in one step.
- Your behaviours do not run while you edit, so a command can find Objs but not their behaviours. While the game plays, it runs against the game instead.

A guide in VoxDocs can run a command from a button. See [[Step-by-Step Guides]].

## Commands while the game runs

In a development build, press the key to the left of 1 to open the game's console, and type a command's method name to run it. There, commands can take values and give one back:

```csharp
[Command("Spawn Wave")]
private static string SpawnWave(int count, string kind)
{
	// ...
	return $"Spawned {count} {kind}";
}
```

Typing `SpawnWave 3 Wolves` runs it and prints what it returns. Commands that take values do not show in the editor's command bar.

## Calls for tools

A **call** is a static method that tools reach: `vox editor call` and `vox mcp` list it beside the editor's own calls, so an AI assistant can drive your game's systems as it drives the engine's. Mark it `[Call]` with its name and what it does:

```csharp
[Call("society.residents", About = "The residents near a place: each one's name, home and what they are doing.")]
private static List<ResidentInfo> Residents(Vector3 near, float reach = 100f)
{
	return Society.Near(near, reach).Select(resident => resident.Info).ToList();
}
```

```bash
vox editor call society.residents near=[120,40,-30] reach=50
```

- **Arguments** are read by their names: text, toggles, numbers, enums by name, vectors as `[x, y, z]`, Objs as tools name them, and anything else as a save writes it. One with a default may be left out.
- **What it returns** comes back as a save writes it: numbers, text, lists, dictionaries, and classes by their fields.
- **When** says when it may be called: while editing and while playing (the default), or only one of them. While editing it runs against the level being edited, the scripts starting first if they have not.
- **Changes = true** marks a call that changes the level. It runs only where the editor's settings let tools change the project, and what it changed while editing is one step to undo.

`vox editor call game.calls` lists them, each with its arguments, and `game.call` makes one by its name.

