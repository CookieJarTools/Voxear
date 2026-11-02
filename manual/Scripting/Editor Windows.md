---
order: 14
---

An **editor window** is a window of your own in the editor: a panel for the tools you build your levels with, such as a checklist of what is still to place, or buttons that line things up. You write it in C#, with the same elements a game's UI is made of ([[UI from Scripts]]).

```csharp
using Voxear;

namespace MyGame;

public sealed class LevelTools : EditorWindow
{
	private Label player = null!;

	protected override void Build(UiElement root)
	{
		player = root.Add(new Label());
		root.Add(new Button("Swap Spinners")).Clicked += Tools.SwapSpinners;
	}

	protected override void Update(UiDocument ui)
	{
		player.Text = Obj.Find("Player").IsValid ? "The player is placed" : "Place the player";
	}
}
```

## Opening an editor window

Open it from any window's menu: click **⋯** at the right end of a window's tabs, or right-click a tab, and pick **Add Level Tools**. It opens beside that window. Pick it again for another: each is a window of its own, with its own fields.

The menu calls a window by its class name in words. To call it something else, and give it an icon, mark it with `[Window]`:

```csharp
[Window("Lamp Tools", Icon = "light")]
public sealed class LevelTools : EditorWindow
```

## What an editor window does

- `Build` runs once, before the window first shows. It adds the window's elements to `root`, and keeps them in fields. A window can describe its elements in `Markup` instead, as a drawer can ([[Custom Drawers]]); `ui.Find` then reaches them in `Update`.
- `Update` runs every frame the window shows, after the `Clicked` and `Changed` of its elements. Use it to keep what the window shows up to date.
- A window works while you edit and while the game plays. While you edit, its code runs on the level you are editing, so `Obj.Find` finds what is in it; while the game plays, it runs on the game.
- `Closed` runs when you close the window, to let go of anything it holds.
- A layout keeps the editor windows it shows, and opens them again with it.
- When your scripts change, each window opens again from the new code. Its fields start again, as a new window's do.

## Adding to the side bar

The side bar is the narrow strip down the editor's left edge, with **Levels** at the top. Your game can add its own buttons there, under Levels.

`[SideBarButton]` on a static method makes a button with an icon that runs it. The title shows when the pointer rests on the button:

```csharp
public static class LevelTools
{
    [SideBarButton(IconName.Grid, "Snap Everything")]
    private static void SnapEverything()
    {
        // Runs as a command does: on the level you are editing, as one step to undo.
    }
}
```

Give several methods the same `Menu` and they share one button, which opens a menu of them:

```csharp
[SideBarButton(IconName.Light, "Light Every Lamp", Menu = "Lamps")]
private static void LightEveryLamp() { }

[SideBarButton(IconName.Light, "Put Every Lamp Out", Menu = "Lamps")]
private static void PutEveryLampOut() { }
```

For more than a button, derive from `SideBarItem`. It is built and updated as an editor window is, with `Build` and `Update`, but it sits in the side bar, under the buttons, so keep it small: a light that says whether a server answers, or a count of something.

```csharp
public sealed class EnemyCount : SideBarItem
{
    private Label count = null!;

    protected override void Build(UiElement root)
    {
        count = root.Add(new Label());
    }

    protected override void Update(UiDocument ui)
    {
        count.Text = Obj.FindAll("Enemy").Length.ToString();
    }
}
```

The side bar is built again whenever your scripts change.
