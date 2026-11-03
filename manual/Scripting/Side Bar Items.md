---
order: 15
---

The **side bar** is the narrow strip down the editor's left edge, with **Levels** at the top. Your game can add its own things there, under Levels, always in view whatever panels are open:

- a **side bar button**, `[SideBarButton]` on a static method, is a button with an icon that runs the method;
- a **side bar item**, a class derived from `SideBarItem`, is a small piece of UI of your own, built and updated as an editor window is.

Use a button for a job you run often while building levels. Use an item for something small you want to see all the time: a light that says whether a server answers, or a count of something. The side bar is narrow, so for anything bigger, such as a list or a form, write an [[Editor Windows|editor window]].

## A Snap Everything button

`[SideBarButton]` takes an icon and a title. The title shows when the pointer rests on the button:

```csharp
using Voxear;

namespace MyGame;

internal static class LevelButtons
{
	[SideBarButton(IconName.Grid, "Snap Everything")]
	private static void SnapEverything()
	{
		// Runs as a command does: on the level you are editing, as one step to undo.
	}
}
```

The method must be `static`, take nothing and return nothing. It runs as a command does ([[Commands]]): on the level you are editing, as one step to undo, or on the game while it plays.

## Buttons that share a menu

Give several methods the same `Menu` and they share one button, with the icon of the first, which opens a menu of them:

```csharp
[SideBarButton(IconName.Light, "Light Every Lamp", Menu = "Lamps")]
private static void LightEveryLamp()
{
}

[SideBarButton(IconName.Light, "Put Every Lamp Out", Menu = "Lamps")]
private static void PutEveryLampOut()
{
}
```

## An Enemy Count item

For more than a button, derive from `SideBarItem`. This one shows how many enemies the level has, under the side bar's buttons:

```csharp
using Voxear;

namespace MyGame;

public sealed class EnemyCount : SideBarItem
{
	protected override void Build(UiElement root)
	{
		root.Add(new Label { Tooltip = "Enemies in the level" }.Bind(() => Obj.FindAll("Enemy").Length.ToString()));
	}
}
```

`Bind` works the label's text out again every frame the item shows, after `Update`. The same item in markup finds its label in `Update` instead:

```csharp
public sealed class EnemyCount : SideBarItem
{
	public override string Markup => """
		<vui>
			<Label name="count" tooltip="Enemies in the level" />
		</vui>
		""";

	protected override void Update(UiDocument ui)
	{
		ui.Find("count")!.Text = Obj.FindAll("Enemy").Length.ToString();
	}
}
```

A side bar item runs while you edit and while the game plays, as an editor window does, and the side bar is built again whenever your scripts change.

## Reference

### SideBarButtonAttribute

| Member | What it does |
| --- | --- |
| `[SideBarButton(IconName icon, string title)]` | Puts a static method on the side bar as a button with that icon |
| `Icon` | The button's icon |
| `Title` | What the button's tooltip says, or its line in a menu |
| `Menu` | The menu it is a line of, shared by every method with the same `Menu`. A button of its own when not given |

### SideBarItem

`SideBarItem` is an `EditorWindow` that sits in the side bar, so it has the same members:

| Member | What it does |
| --- | --- |
| `protected virtual void Build(UiElement root)` | Adds the item's elements to `root`, once, before its first `Update` |
| `protected virtual void Update(UiDocument ui)` | Runs every frame the item shows, after its elements' callbacks. `ui.Find` finds what the markup named |
| `protected virtual void Closed()` | Runs as the scripts reload, to let go of what it holds |
| `public virtual string Markup` | The elements as a `.vui` document. Leave it out to build them in `Build` |
