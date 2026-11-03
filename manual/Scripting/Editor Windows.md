---
order: 15
---

An **editor window** is a window of your own in the editor, as an `EditorWindow` is in Unity: a panel for the tools you build your levels with, such as a checklist of what is still to place, or buttons that line things up. You write it in C#, with the same elements a game's UI is made of ([[UI from Scripts]]).

Use one for tools that are not about one field or one behaviour. To change how a field or a behaviour shows in the Inspector, write a drawer instead ([[Custom Drawers]]). For a single button that runs a method, or a small readout always in view, use the side bar ([[Side Bar Items]]). A job you only run, with nothing to show, can be a command ([[Commands]]).

## A Level Tools window

This window says whether the player has been placed, and has a button that runs the **Swap Spinners** command from [[Commands]]. Mark that method `internal` rather than `private`, so the window can call it:

```csharp
using Voxear;

namespace MyGame;

public sealed class LevelTools : EditorWindow
{
	protected override void Build(UiElement root)
	{
		root.Add(new Label().Bind(() => Obj.Find("Player").IsValid ? "The player is placed" : "Place the player"));
		root.Add(new Button("Swap Spinners", Tools.SwapSpinners) { Tooltip = "Swaps Spinner A and Spinner B" });
	}
}
```

- `Build` runs once, before the window first shows. It adds the window's elements to `root`.
- `Bind` gives the label a function that works out its text. A window works it out again every frame it shows, so the label is always right.
- The button's second argument runs when it is clicked.

## The Level Tools window in markup

A window can describe its elements in `Markup` instead, as a `.vui` document written in the code. `Update` runs every frame the window shows, and `ui.Find` reaches each element by its `name`:

```csharp
using Voxear;

namespace MyGame;

public sealed class LevelTools : EditorWindow
{
	public override string Markup => """
		<vui>
			<Label name="player" />
			<Button name="swap" tooltip="Swaps Spinner A and Spinner B">Swap Spinners</Button>
		</vui>
		""";

	protected override void Update(UiDocument ui)
	{
		if (ui.Find("swap")!.WasClicked)
		{
			Tools.SwapSpinners();
		}
		ui.Find("player")!.Text = Obj.Find("Player").IsValid ? "The player is placed" : "Place the player";
	}
}
```

`WasClicked` says whether the button was clicked since the last frame. Sheets the markup's `<Style src>` names are read from the project, by their path in it.

## Opening an editor window

Open it from any window's menu: click **⋯** at the right end of a window's tabs, or right-click a tab, and pick **Add Level Tools**. It opens beside that window. Pick it again for another: each is a window of its own, with its own fields.

## Naming an editor window with [Window]

The menu calls a window by its class name in words, so `LevelTools` is **Level Tools**. To call it something else, and give it an icon, mark it with `[Window]`:

```csharp
[Window("Lamp Tools", Icon = IconName.Light)]
public sealed class LevelTools : EditorWindow
```

`Icon` is one of Voxear's icons, by its name in `IconName`, as `[SideBarButton]` takes one. Without it, the window has the editor's icon for a window.

## How an editor window runs

Each frame the window shows:

1. On the first frame, `Build` runs. On later frames, the callbacks of whatever was clicked or changed run, and its `Clicked` and `Changed` events.
2. `Update` runs.
3. Every `Bind` is worked out again.

- A window works while you edit and while the game plays. While you edit, its code runs on the level you are editing, so `Obj.Find` finds what is in it; while the game plays, it runs on the game.
- `Closed` runs when you close the window, and when your scripts reload, to let go of anything it holds.
- A layout keeps the editor windows it shows, and opens them again with it.
- When your scripts change, each window opens again from the new code. Its fields start again, as a new window's do.
- A tooltip given to an element shows under it once the pointer rests there for half a second.

## Reference

### EditorWindow

| Member | What it does |
| --- | --- |
| `protected virtual void Build(UiElement root)` | Adds the window's elements to `root`, once, before its first `Update` |
| `protected virtual void Update(UiDocument ui)` | Runs every frame the window shows, after its elements' callbacks. `ui.Find` finds what the markup named |
| `protected virtual void Closed()` | Runs as the window closes, or as the scripts reload |
| `public virtual string Markup` | The elements as a `.vui` document. Leave it out to build them in `Build` |

### WindowAttribute

| Member | What it does |
| --- | --- |
| `[Window(string title)]` | Names the window in the editor's menus and on its tab |
| `Title` | The name it was given |
| `Icon` | The icon beside the name, an `IconName`. The editor's icon for a window when none is given |
