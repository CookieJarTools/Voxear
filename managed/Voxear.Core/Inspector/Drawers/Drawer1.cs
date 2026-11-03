namespace Voxear;

/// <summary>
/// What ValueDrawer and AttributeDrawer share: the field's value, as Value, and the calls that show it. A drawer
/// built in Build with handlers that set Value needs nothing else; Refresh and Draw are there when it does.
/// </summary>
public abstract class Drawer<T> : Drawer
{
	private protected Drawer()
	{
	}

	/// <summary>
	/// The field's value as the drawer draws it, from the start of Build on. A handler that sets it, as a button's
	/// <c>() => Value = Value with { Current = Value.Max }</c>, changes the field: the value is saved in the scene
	/// once the handlers have run, as one step to undo. Set it from handlers, Build, Refresh or Draw; set later, as
	/// after an await, it is not saved.
	/// </summary>
	protected T Value { get; set; } = default!;

	/// <summary>
	/// Keeps the elements in step with the value: runs on the first draw, and whenever the value is not the one the
	/// elements last showed, because it changed from outside, as on undo or from a script, or because a handler
	/// set it. Elements given Bind keep themselves up to date without it.
	/// </summary>
	protected virtual void Refresh(T value)
	{
	}

	/// <summary>
	/// The advanced path, as drawers were first written: shows the value in the elements and gives it back as what
	/// was done to them leaves it. It runs on every draw, after the handlers, so an element's WasClicked says
	/// whether it was clicked. A value given back changed is saved, as one step to undo.
	/// </summary>
	protected virtual T Draw(T value, UiDocument ui)
	{
		return value;
	}

	internal sealed override void Take(object? value)
	{
		Value = (T)value!;
	}

	internal sealed override object? DrawValue(UiDocument ui)
	{
		Value = Draw(Value, ui);
		return Value;
	}

	internal sealed override void RefreshValue(object? value)
	{
		Refresh((T)value!);
	}
}
