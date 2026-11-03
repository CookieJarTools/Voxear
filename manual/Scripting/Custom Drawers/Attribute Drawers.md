---
order: 2
---

An **attribute drawer** draws only the fields you mark with an attribute of your own, such as `[Step(5)]`. Write an attribute, then derive a drawer for it from `AttributeDrawer<TAttribute, T>`, where `T` is the type of field it draws. Use one for a look you want on some fields and not others: buttons beside a number, a line of help, a warning colour.

When every field of a type should look the same, write a [[Type Drawers|type drawer]] instead. When an attribute Voxear already has will do, such as `[Range]` or `[InfoBox]`, use that ([[Fields in the Inspector]]).

## A Step drawer for whole numbers

This drawer puts a **-** and a **+** either side of a whole number, each changing it by the step the attribute is given. First the attribute:

```csharp
using System;

namespace MyGame;

[AttributeUsage(AttributeTargets.Field)]
public sealed class StepAttribute(int step) : Attribute
{
	public int Step { get; } = step;
}
```

Then the drawer, with the field's usual number box between its buttons:

```csharp
using Voxear;

namespace MyGame;

public sealed class StepDrawer : AttributeDrawer<StepAttribute, int>
{
	protected override void Build(UiElement root)
	{
		root.Add(new Button("-", () => Value -= Attribute.Step));
		root.Add(new NextField());
		root.Add(new Button("+", () => Value += Attribute.Step));
	}
}
```

Mark a field with it:

```csharp
[Step(5)]
public int Lives = 3;
```

![A whole number field before its Step drawer, as a plain number box](images/step-before.png)

![The same field drawn by StepDrawer, with a - button and a + button either side of its number box](images/step-after.png)

- `Attribute` is the attribute on the field, with what it was given, so `Attribute.Step` is 5 here.
- `Value` is the field's value. Each button sets it, and the change is saved in the scene as one step for **Ctrl+Z** to undo.
- `NextField` is where the field's usual number box goes. Without it, the drawer would draw the field entirely itself.

## The Step drawer in markup

In markup, an element named `next` takes the place of the `NextField`. A `Panel` is usual, as it holds whatever goes in it:

```csharp
using Voxear;

namespace MyGame;

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

`Draw` runs on every draw, after the handlers. It gives back the value, changed by whatever was clicked, and a changed value is saved as one step to undo. [[Type Drawers]] says more about `Draw`, `Refresh` and when each runs; they work the same in an attribute drawer.

## A drawer for fields of any type

`AttributeDrawer<TAttribute>`, with no type, draws every field marked with the attribute, whatever its type. Its `Value` is an `object`, holding a value of the field's own type. It suits a drawer that adds something around the field and leaves the value to the usual control in its `NextField`:

```csharp
using System;
using Voxear;

namespace MyGame;

[AttributeUsage(AttributeTargets.Field)]
public sealed class HelpAttribute(string text) : Attribute
{
	public string Text { get; } = text;
}

public sealed class HelpDrawer : AttributeDrawer<HelpAttribute>
{
	protected override void Build(UiElement root)
	{
		var help = new Label(Attribute.Text) { Visible = false };
		root.Add(new Button("?", () => help.Visible = !help.Visible));
		root.Add(new NextField());
		root.Add(help);
	}
}
```

`[Help("Full at the start of each level")]` now puts a **?** beside any field, which shows that line under it.

## Attribute drawers on a list

On a list or array, the drawer draws each item: `[Step(10)] public List<int> Scores = [];` puts **-** and **+** beside every score.

## Several drawers on one field

A field with two such attributes is drawn by both: the first written is outermost, and the second goes in its `NextField`. A type drawer for the field's type comes inside those. [[Custom Drawers]] shows a field drawn by an attribute drawer and a type drawer together.

## Reference

`AttributeDrawer<TAttribute, T>` is a `Drawer<T>`, as a type drawer is, with `Attribute` added.

| Member | What it does |
| --- | --- |
| `AttributeDrawer<TAttribute, T>` | Draws the fields marked with `TAttribute` whose values are `T`, and each item of a list marked with it |
| `AttributeDrawer<TAttribute>` | Draws the fields marked with `TAttribute`, of any type; `Value` is an `object` |
| `public TAttribute Attribute` | The attribute on the field, with what it was given |
| `protected virtual void Build(UiElement root)` | Adds the elements to `root`, once, before the first draw, when `Value` already holds the field's value |
| `protected T Value { get; set; }` | The field's value. Set by a handler, it is saved in the scene as one step to undo |
| `protected virtual void Refresh(T value)` | Brings the elements up to date. Runs on the first draw, and when the value changed since the elements last showed it |
| `protected virtual T Draw(T value, UiDocument ui)` | The advanced path. Runs every draw, after the handlers; gives back the value, saved when changed |
| `public virtual string Markup` | The elements as a `.vui` document. Leave it out to build them in `Build` |
| `public string Label` | What the Inspector calls the field |
