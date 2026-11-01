using System;

namespace Voxear
{
	/// <summary>
	/// Draws a field's value in the Inspector with Voxear UI, the UI games draw theirs with. Derive from ValueDrawer to
	/// draw every field of a type, or from AttributeDrawer to draw the fields marked with an attribute of your own. The
	/// field keeps its label and the attributes around it, so an InfoBox above a drawn field still shows. Drawers run
	/// in edit mode; while the game plays, the field shows its own control.
	/// </summary>
	public abstract class Drawer
	{
		internal Drawer()
		{
		}

		/// <summary>
		/// The elements the value is drawn with, as a .vui document holds them: <c>&lt;vui&gt;…&lt;/vui&gt;</c>. The sheets its
		/// Style elements name are read from the project. An element named "next" is where the field's next drawer
		/// draws: the drawer for its type inside the one for its attribute, or the Inspector's own control. Without
		/// one, this drawer draws the value alone.
		/// </summary>
		public abstract string Markup { get; }

		/// <summary>What the Inspector calls the field.</summary>
		public string Label { get; internal set; } = string.Empty;

		internal virtual void Attach(Attribute attribute)
		{
		}

		internal abstract object? DrawValue(object? value, UiDocument ui);
	}
}
