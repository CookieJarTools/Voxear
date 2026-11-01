using System;

namespace Voxear
{
	/// <summary>
	/// Draws the fields marked with TAttribute whose values are T, and each item of a list marked with it:
	/// <c>class MinMaxDrawer : AttributeDrawer&lt;MinMaxAttribute, Vector2&gt;</c>. A field marked with two such attributes
	/// is drawn by both, the first written outside the second, through the "next" element of its markup.
	/// </summary>
	public abstract class AttributeDrawer<TAttribute, T> : Drawer
		where TAttribute : Attribute
	{
		/// <summary>The attribute on the field, with what it was given.</summary>
		public TAttribute Attribute { get; private set; } = null!;

		/// <summary>
		/// Shows the value in the elements, and gives it back as what was done to them leaves it, as ValueDrawer's
		/// Draw does.
		/// </summary>
		protected abstract T Draw(T value, UiDocument ui);

		internal sealed override void Attach(System.Attribute attribute)
		{
			Attribute = (TAttribute)attribute;
		}

		internal sealed override object? DrawValue(object? value, UiDocument ui)
		{
			return Draw((T)value!, ui);
		}
	}
}
