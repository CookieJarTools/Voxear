using System;

namespace Voxear;

/// <summary>
/// Draws a field's value in the Inspector with Voxear UI, the UI games draw theirs with. Derive from ValueDrawer to
/// draw every field of a type, or from AttributeDrawer to draw the fields marked with an attribute of your own. The
/// field keeps its label and the attributes around it, so an InfoBox above a drawn field still shows. Drawers run
/// in edit mode; while the game plays, the field shows its own control.
/// <para>
/// Build the drawer's elements in code, as Unity's UI Toolkit does, by adding them to the root in Build and keeping
/// them in fields; or describe them in Markup, which can also hold a "next" element for the Inspector's own control.
/// </para>
/// <code>
/// protected override void Build(UiElement root)
/// {
///     bar = root.Add(new Slider(0, 100));
///     text = root.Add(new Label());
/// }
/// </code>
/// </summary>
public abstract class Drawer
{
	private UiElement? root;

	internal Drawer()
	{
	}

	/// <summary>
	/// The elements the value is drawn with, as a .vui document holds them: <c>&lt;vui&gt;…&lt;/vui&gt;</c>. The sheets its
	/// Style elements name are read from the project. An element named "next" is where the field's next drawer
	/// draws: the drawer for its type inside the one for its attribute, or the Inspector's own control. Without
	/// one, this drawer draws the value alone. Leave it out to build the elements in code, in Build.
	/// </summary>
	public virtual string Markup => string.Empty;

	/// <summary>What the Inspector calls the field.</summary>
	public string Label { get; internal set; } = string.Empty;

	/// <summary>
	/// Adds the elements the value is drawn with to `root`, once, before the first draw. Keep them in fields to read
	/// and write them in Draw. A drawer built in code draws the value alone: for the Inspector's own control inside
	/// it, use Markup with a "next" element.
	/// </summary>
	protected virtual void Build(UiElement root)
	{
	}

	internal virtual void Attach(Attribute attribute)
	{
	}

	/// <summary>The markup the editor builds, an empty document when the drawer builds in code.</summary>
	internal string MarkupToBuild => Markup.Length > 0 ? Markup : "<vui></vui>";

	/// <summary>Builds it the first time, and raises what was clicked or changed in what it built, then draws.</summary>
	internal object? Draw(object? value, UiDocument ui)
	{
		if (root == null)
		{
			root = ui.Root;
			Build(root);
		}
		else
		{
			root.DispatchTree();
		}
		return DrawValue(value, ui);
	}

	internal abstract object? DrawValue(object? value, UiDocument ui);
}
