---
order: 5
---

# Commands

A command is a static method that the editor's command bar (Ctrl+K) can run. Use commands for jobs you do again and again while building levels, such as lining up a row of lamps or renaming a batch of Objs.

```csharp
using Voxear;

namespace MyGame
{
	internal static class Tools
	{
		[Command("Swap Spinners", Keys = "Ctrl+Shift+X")]
		private static void SwapSpinners()
		{
			var a = Obj.Find("Spinner A");
			var b = Obj.Find("Spinner B");
			if (!a.IsValid || !b.IsValid)
			{
				Log.Warning("Swap Spinners needs Objs called Spinner A and Spinner B.");
				return;
			}

			(a.Position, b.Position) = (b.Position, a.Position);
		}
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
