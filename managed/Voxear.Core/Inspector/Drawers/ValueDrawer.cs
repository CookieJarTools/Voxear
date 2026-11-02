namespace Voxear;

/// <summary>
/// Draws every field of type T in the Inspector, and each item of a list of them, with the elements Markup
/// describes. <c>class HealthDrawer : ValueDrawer&lt;Health&gt;</c> with a Draw that writes the value into its labels
/// and reads what was clicked.
/// </summary>
public abstract class ValueDrawer<T> : Drawer
{
	/// <summary>
	/// Shows the value in the elements, and gives it back as what was done to them leaves it. It is called when the
	/// field is first drawn, when its value changes elsewhere, as on undo, and after something in it was clicked or
	/// changed, so an element's WasClicked says whether that was it. A value given back changed is one step to undo.
	/// </summary>
	protected abstract T Draw(T value, UiDocument ui);

	internal sealed override object? DrawValue(object? value, UiDocument ui)
	{
		return Draw((T)value!, ui);
	}
}
