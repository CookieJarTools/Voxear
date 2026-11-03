using System.Linq;
using Voxear.Scripting;

namespace Voxear;

/// <summary>
/// Draws a whole behaviour in the Inspector, in place of its fields, as Unity's custom Editor does:
/// <c>class TowerInspector : BehaviourInspector&lt;Tower&gt;</c>. It draws behaviours of type T and of types derived
/// from it, unless they have an inspector of their own. Build its elements in Build, with a NextField where the
/// behaviour's usual fields go, or describe them in Markup, with an element named "next".
/// <para>
/// Target is a copy of the behaviour made from the scene's values, on its Obj, and whatever the handlers change in
/// its fields is saved in the scene, as one step to undo. It runs in edit mode; while the game plays, the behaviour
/// shows its fields.
/// </para>
/// <code>
/// protected override void Build(UiElement root)
/// {
///     root.Add(new Label().Bind(() => $"{Target.Floors} floors on {Target.Obj.Name}"));
///     root.Add(new Button("Add a floor", () => Target.Floors++));
///     root.Add(new NextField());
/// }
/// </code>
/// </summary>
public abstract class BehaviourInspector<T> : Drawer
	where T : Behaviour
{
	/// <summary>
	/// The behaviour as the scene has it, a copy made again for each draw, on its Obj: read it in bindings and
	/// handlers, and change its fields there to change the scene's.
	/// </summary>
	protected T Target { get; private set; } = null!;

	/// <summary>
	/// Keeps the elements in step with the behaviour: runs on the first draw, and whenever its fields are not as the
	/// elements last showed them, because they changed from outside, as on undo, or a handler changed them.
	/// </summary>
	protected virtual void Refresh(T behaviour)
	{
	}

	/// <summary>
	/// The advanced path: shows the behaviour in the elements, and changes its fields as what was clicked or changed
	/// in them asks. It runs on every draw, after the handlers, so an element's WasClicked says whether that was it.
	/// </summary>
	protected virtual void Draw(T behaviour, UiDocument ui)
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
		var fields = InspectorCalls.Values((Behaviour)value!);
		return string.Join('\n', fields.Select(field => $"{field.Key}={field.Value}"));
	}
}
