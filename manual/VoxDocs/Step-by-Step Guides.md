---
order: 3
---

A guide is a page that walks someone through a task, such as setting up a new level or adding an enemy. Its steps can do part of the work for the reader, and tick themselves off once the work is done.

A step is a link on a line of its own. There are two kinds:

- An **action** is a button that runs a command.
- A **check** ticks itself while what it looks for is there, and says what is still missing.

A page with steps shows how many are done at the top, with **Reset** to start again.

## A step to try

This adds a point light to the level you have open. Undo takes it away again.

[Add a point light](<action:New Point Light>)

This step ticks itself as soon as the light is there:

[There is a Point Light](<check:obj/Point Light>)

## Actions

```markdown
[Add a point light](<action:New Point Light>)
```

The part after `action:` is a command's title, exactly as the command bar (Ctrl+K) lists it. That can be one of the editor's own commands, such as `Play`, or one your scripts add (see [[Commands]]). An action runs on the level you are editing, and Undo takes back what it did.

Put the address in angle brackets, as above, when the title has spaces in it.

An action can also make an asset, as the Project panel's **New** menu does:

```markdown
[Make a material](action:create/Material)
[Make the brass material](<action:create/Material?at=Materials/Brass.material>)
```

The first makes a material next to your others. The second makes it at that exact path, or finds it there if it is already made. Folders, scenes, levels and scripts can be made the same way.

## Checks

| Write | Ticks while |
| --- | --- |
| `[words](check:obj/Sun)` | there is an Obj called Sun |
| `[words](check:component/Light)` | some Obj has a Light, or a behaviour called Light |
| `[words](check:file/Prefabs/Lamp.prefab)` | the project has that file |
| `[words](check:code/MyGame.Tools.LampsAreLit)` | a check in your C# code passes |

## Checks in C#

When none of these fit, write the check in C#. Make it a static method that takes nothing, mark it with `[DocsCheck]`, and return `true` when the step is done:

```csharp
using Voxear;

namespace MyGame
{
	internal static class Tools
	{
		[DocsCheck("The sun is up")]
		private static string? SunIsUp()
		{
			var sun = Obj.Find("Sun");
			if (!sun.IsValid)
			{
				return "Add an Obj called Sun.";
			}

			return sun.Position.Y > 0 ? null : "Move the Sun up.";
		}
	}
}
```

Return `bool` for a plain yes or no. Return `string` to say what is still to do: `null` means the step is done, and any other text shows under the step. Link to it with the type's full name and the method's name:

```markdown
[The sun is up](check:code/MyGame.Tools.SunIsUp)
```

The check runs about once a second against the level you are editing, or against the game while it plays. Your scripts need to build first: until they do, the step says it is waiting for them.

## Your progress

Ticked steps stay ticked on your own machine, through restarts. They are kept in the project's `.voxear` folder, so they are not shared with your team.

While editing a page, **Action…** lists the commands, the kinds of check and the C# checks your scripts have, and puts in the one you pick.

Next: [[Pages and Code]].
