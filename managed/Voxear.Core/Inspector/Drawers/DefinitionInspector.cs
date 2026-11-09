using System.Linq;
using Voxear.Scripting;

namespace Voxear;

/// <summary>
/// Draws a whole data asset in the Inspector, in place of its Definition's fields, as a BehaviourInspector draws a
/// behaviour: <c>class WeaponStatsInspector : DefinitionInspector&lt;WeaponStats&gt;</c>. It draws data assets of
/// type T and of types derived from it, unless they have an inspector of their own. Build its elements in Build, with
/// a NextField where the usual fields go, or a FieldRow for one of them, or describe them in Markup.
/// <para>
/// Target is a copy of the data asset made from its file, and whatever the handlers change in its fields is written
/// to the file at once. It runs in edit mode; while the game plays, the data asset shows its fields.
/// </para>
/// <code>
/// protected override void Build(UiElement root)
/// {
///     root.Add(new Label().Bind(() => $"{Target.Name}: {Target.Damage} damage"));
///     root.Add(new Button("Double", () => Target.Damage *= 2f));
///     root.Add(new FieldRow(nameof(WeaponStats.Shots)));
///     root.Add(new NextField());
/// }
/// </code>
/// </summary>
public abstract class DefinitionInspector<T> : Drawer
	where T : Definition
{
	/// <summary>
	/// The data asset as its file has it, a copy made again for each draw: read it in bindings and handlers, and
	/// change its fields there to change the file's. Its Name is the file's.
	/// </summary>
	protected T Target { get; private set; } = null!;

	/// <summary>
	/// Keeps the elements in step with the data asset: runs on the first draw, and whenever its fields are not as the
	/// elements last showed them, because they changed from outside or a handler changed them.
	/// </summary>
	protected virtual void Refresh(T definition)
	{
	}

	/// <summary>
	/// The advanced path: shows the data asset in the elements, and changes its fields as what was clicked or changed
	/// in them asks. It runs on every draw, after the handlers, so an element's WasClicked says whether that was it.
	/// </summary>
	protected virtual void Draw(T definition, UiDocument ui)
	{
	}

	internal sealed override void Take(object? value)
	{
		Target = (T)value!;
	}

	internal sealed override object? DrawValue(UiDocument ui)
	{
		Draw(Target, ui);
		return Target;
	}

	internal sealed override void RefreshValue(object? value)
	{
		Refresh((T)value!);
	}

	internal override string Written(object? value)
	{
		var fields = InspectorCalls.Values(value!);
		return string.Join('\n', fields.Select(field => $"{field.Key}={field.Value}"));
	}
}
