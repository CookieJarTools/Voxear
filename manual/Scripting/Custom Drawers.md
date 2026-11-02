---
order: 9
---

When no attribute shows a field the way you want, write a **drawer**: a class that draws the field in the Inspector. It is built from the same elements and calls as a game's own UI ([[UI from Scripts]]).

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
	private Label text = null!;
	private Button fill = null!;

	protected override void Build(UiElement root)
	{
		text = root.Add(new Label());
		fill = root.Add(new Button("Fill"));
	}

	protected override Health Draw(Health value, UiDocument ui)
	{
		if (fill.WasClicked)
		{
			value.Current = value.Max;
		}
		text.Text = $"{value.Current} / {value.Max}";
		return value;
	}
}
```

Every `Health` field now shows as its two numbers and a **Fill** button, in place of its own fields. In a list of `Health`, each item shows that way.

`Build` runs once, before the field is first drawn. It adds the drawer's elements to `root`, as a game builds its UI in code, and keeps them in fields.

`Draw` gets the value and the document. It writes the value into the elements, and gives the value back, changed by whatever was clicked. The editor calls it when the field first shows, after something in it is clicked or changed, and when the value changes some other way, as on undo. A value given back changed is saved in the scene, and **Ctrl+Z** undoes it.

## A drawer from markup

A drawer can describe its elements in markup instead, as a `.vui` document written in the code. A raw string, between `"""` marks, keeps it readable, and `Find` reaches each element by its `name`:

```csharp
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

Sheets the markup's `<Style src>` names are read from the project, by their path in it. Markup is also the way to keep the field's usual control inside your drawer, with an element named `next`, as the next section shows. A drawer built in code always draws the whole field itself.

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

## An inspector for a whole behaviour

To draw a whole behaviour your own way, in place of its list of fields, write a `BehaviourInspector` for it, as you would write a custom `Editor` in Unity:

```csharp
public sealed class Tower : Behaviour
{
	public int Floors = 3;
	public float Height;
}

public sealed class TowerInspector : BehaviourInspector<Tower>
{
	public override string Markup => """
		<vui>
			<Label name="summary" />
			<Button name="raise">Add a floor</Button>
			<Panel name="next" />
		</vui>
		""";

	protected override void Draw(Tower tower, UiDocument ui)
	{
		if (ui.Find("raise")!.WasClicked)
		{
			tower.Floors++;
			tower.Height = tower.Floors * 2.5f;
		}
		ui.Find("summary")!.Text = $"{tower.Floors} floors on {tower.Obj.Name}";
	}
}
```

The Tower's section now says how many floors it has, with an **Add a floor** button, and its usual fields below them.

- `Draw` gets a copy of the behaviour made from the scene's values, on its Obj, so `tower.Obj` is the Obj it is on. Whatever `Draw` changes in the copy's fields is saved in the scene, all of it as one step for **Ctrl+Z** to undo.
- The element named `next` is where the behaviour's usual fields and buttons go. Leave it out to draw the behaviour entirely yourself. As with drawers, you can build the elements in `Build` instead of writing markup.
- An inspector also draws the behaviours derived from its type, unless they have an inspector of their own.

## How drawers behave

- A drawn field keeps its label, and the attributes around it still apply, so `[InfoBox]`, `[Tooltip]` and groups work as before.
- A field can have several drawers: one for each of its attributes, in the order they are written, then the one for its type. Each is drawn inside the `next` element of the one before.
- Drawers and inspectors run while you edit. While the game plays, a drawn field shows its usual control, and a behaviour with an inspector shows its usual fields.
- In a drawer, check `WasClicked` and `WasChanged` in `Draw`. Events such as `Clicked` and `Moved` are raised too, for elements built in code, just before `Draw` runs.
- A drawer that throws stops drawing its field until you select the Obj again; the Console shows what went wrong.

Next: [[Your First Menu]], to build a game's own UI with the same markup.
