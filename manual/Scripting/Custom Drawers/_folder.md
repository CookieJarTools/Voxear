---
order: 9
---

A **drawer** is a class of your own that draws a field in the Inspector your way, as a property drawer does in Unity. Write one when no attribute shows a field as you want ([[Fields in the Inspector]] lists them): a bar for a health value, buttons beside a number, a summary at the top of a behaviour. To change only a field's name, range or place, use an attribute instead; to make a panel of tools that is not about one Obj, use an [[Editor Windows|editor window]].

A drawer is built from the same elements as a game's UI ([[UI from Scripts]]), and runs only in the editor.

## The three kinds of drawer

| Kind | Derive from | Draws | Use it for |
| --- | --- | --- | --- |
| Type drawer | `ValueDrawer<T>` | Every field of type `T`, and each item of a list of them | A struct or class of your own that should always look the same way |
| Attribute drawer | `AttributeDrawer<TAttribute, T>` | Only the fields you mark with an attribute of your own | A look you want on some fields, such as **-** and **+** buttons beside a number |
| Behaviour inspector | `BehaviourInspector<T>` | A whole behaviour's section, in place of its list of fields | Buttons and summaries for a behaviour as a whole, as a custom `Editor` in Unity |
| Data asset inspector | `DefinitionInspector<T>` | A whole data asset, in place of its list of fields | The same for a data asset |

Each kind has a page of its own, with a worked example:

1. [[Type Drawers]]
2. [[Attribute Drawers]]
3. [[Behaviour Inspectors]]
4. [[Data Asset Inspectors]]

## Why a drawer has Build and Draw

A drawer's elements are made once, in `Build`, and kept. After that, only what changes is changed: a label's text, a slider's value. This is called **retained** UI, and it is how Unity's UI Toolkit works. Unity's older IMGUI, with its `OnGUI`, works the other way: it draws everything again from nothing every frame.

Keeping the elements has three gains:

- A text field keeps its caret and what is selected in it while you type.
- A foldout stays open or closed as you left it.
- It is cheap: nothing is made again when nothing has changed.

So a drawer has two steps. `Build` makes the elements. Then `Bind`, `Refresh` or `Draw` bring them up to date whenever the value changes, whether from your own buttons, from undo or from a script.

## Why Bind takes a function

```csharp
root.Add(new Label().Bind(() => $"{Value.Current} / {Value.Max}"));
```

`Bind` is given a function, `() => …`, not a value. The drawer calls the function again each time it brings its elements up to date, so the label always shows the value as it is then. Given a plain value, as `Bind($"{Value.Current}")` would be, the label could only read it once, when it was made, and would stay as it was.

## Where the usual control goes

An element named `next` in markup, or a `NextField` added in code, marks where whatever would normally show goes:

- the field's next drawer, when it has more than one;
- otherwise the field's usual control, such as its number box;
- for a behaviour inspector, the behaviour's usual fields and buttons.

With one, your drawer **wraps** what was there: a button beside the usual number box, say. Without one, it **replaces** it, and draws the value entirely itself. It is Unity's `PropertyField` inside a `PropertyDrawer`, or `DrawDefaultInspector` in a custom `Editor`, given as a place in your elements rather than as a call.

Add the `NextField` once, in `Build`, and keep it: what goes into it is put there after the drawer's first draw, and goes with it if you remove it.

## Drawers stacking on one field

A field can have several drawers. Its attribute drawers come first, in the order the attributes are written, the first outermost. Then comes the drawer for its type. Each is drawn inside the `NextField` (or `next`) of the one before, and the usual control goes in the innermost.

Here a field has an attribute drawer and a type drawer. `HelpDrawer` works for fields of any type, and adds a **?** button that shows a line of help:

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

public sealed class Player : Behaviour
{
	[Help("Full at the start of each level")]
	public Health Health = new() { Current = 3, Max = 10 };
}
```

With the `HealthDrawer` from [[Type Drawers]], the **Health** field is drawn like this:

- `HelpDrawer`: the **?** button, then its `NextField`, then the help line
  - in the `NextField`, `HealthDrawer`: the numbers and a **Fill** button

`HealthDrawer` has no `NextField`, so it replaces Health's usual **Current** and **Max** boxes. Give it one and they would show inside it.

## How drawers behave

- Drawers run while you edit. While the game plays, a drawn field shows its usual control, and a behaviour with an inspector shows its usual fields.
- A drawn field keeps its label, and the attributes around it still apply, so `[InfoBox]`, `[Tooltip]` and groups work as before.
- A change a drawer makes is saved in the scene, and **Ctrl+Z** undoes it in one step.
- A drawer that throws stops drawing its field until you select the Obj again. The Console shows what went wrong.
- A tooltip given to a drawer's element, `Tooltip = "Fills the bar"`, shows under it once the pointer rests there for half a second.

Next: [[Your First Menu]], to build a game's own UI with the same elements.
