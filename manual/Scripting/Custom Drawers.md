---
order: 9
---

When no attribute shows a field the way you want, write a **drawer**: a class that draws the field in the Inspector. It is built from the same markup and calls as a game's own UI ([[UI Markup]], [[UI from Scripts]]).

## A drawer for a type

```csharp
[Serializable]
public struct Health
{
	public int Current;
	public int Max;
}

public sealed class HealthDrawer : ValueDrawer<Health>
{
	public override string Markup => """
		<vui>
			<Label name="text" />
			<Button name="fill">Fill</Button>
		</vui>
		""";

	protected override Health Draw(Health value, UiDocument ui)
	{
		if (ui.Find("fill")!.WasClicked)
		{
			value.Current = value.Max;
		}
		ui.Find("text")!.Text = $"{value.Current} / {value.Max}";
		return value;
	}
}
```

Every `Health` field now shows as its two numbers and a **Fill** button, in place of its own fields. In a list of `Health`, each item shows that way.

`Markup` is a `.vui` document, written in the code; a raw string, between `"""` marks, keeps it readable. Sheets its `<Style src>` names are read from the project, by their path in it.

`Draw` gets the value and the document. It writes the value into the elements, and gives the value back, changed by whatever was clicked. The editor calls it when the field first shows, after something in it is clicked or changed, and when the value changes some other way, as on undo. A value given back changed is saved in the scene, and **Ctrl+Z** undoes it.

## A drawer for an attribute

To draw only the fields you mark, write an attribute and an `AttributeDrawer` for it:

```csharp
[AttributeUsage(AttributeTargets.Field)]
public sealed class StepAttribute : Attribute
{
	public StepAttribute(int step)
	{
		Step = step;
	}

	public int Step { get; }
}

public sealed class StepDrawer : AttributeDrawer<StepAttribute, int>
{
	public override string Markup => """
		<vui>
			<Button name="down">-</Button>
			<Panel name="next" />
			<Button name="up">+</Button>
		</vui>
		""";

	protected override int Draw(int value, UiDocument ui)
	{
		if (ui.Find("up")!.WasClicked)
		{
			value += Attribute.Step;
		}
		if (ui.Find("down")!.WasClicked)
		{
			value -= Attribute.Step;
		}
		return value;
	}
}
```

With `[Step(5)] public int Lives = 3;`, the field shows its usual number box with a **-** and a **+** either side.

- `Attribute` is the attribute on the field, with what it was given.
- The element named `next` is where the field's usual control goes. A drawer without one draws the whole field itself.
- `AttributeDrawer<TAttribute, T>` draws fields of type `T`. `AttributeDrawer<TAttribute>`, with no type, draws any field with the attribute, and gets its value as an `object`.

## How drawers behave

- A drawn field keeps its label, and the attributes around it still apply, so `[InfoBox]`, `[Tooltip]` and groups work as before.
- A field can have several drawers: one for each of its attributes, in the order they are written, then the one for its type. Each is drawn inside the `next` element of the one before.
- Drawers run while you edit. While the game plays, a drawn field shows its usual control.
- In a drawer, check `WasClicked` in `Draw`. The `Clicked` event is not raised there.
- A drawer that throws stops drawing its field until you select the Obj again; the Console shows what went wrong.

Next: [[Your First Menu]], to build a game's own UI with the same markup.
