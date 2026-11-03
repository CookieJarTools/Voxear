using System;

namespace Voxear;

/// <summary>
/// Draws the fields marked with TAttribute whose values are T, and each item of a list marked with it:
/// <c>class StepDrawer : AttributeDrawer&lt;StepAttribute, int&gt;</c>. Attribute is the attribute on the field. A
/// NextField among its elements is where the field's usual control goes, or the drawer for its type. A field
/// marked with two such attributes is drawn by both, the first written outside the second, in its NextField.
/// <code>
/// protected override void Build(UiElement root)
/// {
///     root.Add(new Button("-", () => Value -= Attribute.Step));
///     root.Add(new NextField());
///     root.Add(new Button("+", () => Value += Attribute.Step));
/// }
/// </code>
/// </summary>
public abstract class AttributeDrawer<TAttribute, T> : Drawer<T>
	where TAttribute : Attribute
{
	/// <summary>The attribute on the field, with what it was given.</summary>
	public TAttribute Attribute { get; private set; } = null!;

	internal sealed override void Attach(System.Attribute attribute)
	{
		Attribute = (TAttribute)attribute;
	}
}
