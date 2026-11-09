---
order: 4
---

A **data asset inspector** draws a whole data asset in the Inspector, in place of its list of fields, as a [[Behaviour Inspectors|behaviour inspector]] draws a behaviour. Derive from `DefinitionInspector<T>`, where `T` is the Definition (see [[Data Assets]]).

```csharp
using Voxear;

namespace MyGame;

public sealed class WeaponStatsInspector : DefinitionInspector<WeaponStats>
{
	protected override void Build(UiElement root)
	{
		root.Add(new Label().Bind(() => $"{Target.Name}: {Target.Damage} damage"));
		root.Add(new Button("Double", () => Target.Damage *= 2f));
		root.Add(new FieldRow(nameof(WeaponStats.Shots)));
		root.Add(new NextField());
	}
}
```

- `Target` is a copy of the data asset, made from its file for each draw. Its `Name` is the file's.
- What a handler changes in `Target`'s fields is written to the file at once, as an edit in its fields is.
- `Build`, `Refresh`, `Draw` and `Markup` work as they do in a behaviour inspector.

## One field's rows in your own layout

A `FieldRow` is where one of the usual fields goes: its label and control, and for a list its items, or for a `[Serializable]` value the fields under it. Name the field as the script declares it, and a field inside a `[Serializable]` value after a dot, as `"Recoil.Kick"`. The `NextField` gets the fields no `FieldRow` takes.

In markup, the same is an element of its own:

```xml
<vui>
	<Label name="summary" />
	<FieldRow field="Shots" />
	<Panel name="next" />
</vui>
```

A `FieldRow` works the same in a behaviour inspector.

## Reference

| Member | What it does |
| --- | --- |
| `DefinitionInspector<T>` | Draws data assets of type `T`, and of types derived from it without an inspector of their own |
| `protected T Target { get; }` | A copy of the data asset made from its file for each draw. Fields changed in handlers are written to the file |
| `protected virtual void Refresh(T definition)` | Brings the elements up to date when the fields changed |
| `protected virtual void Draw(T definition, UiDocument ui)` | The advanced path. Runs every draw, after the handlers |
| `FieldRow(string field)` | Where one field's rows go, in a behaviour or data asset inspector |
