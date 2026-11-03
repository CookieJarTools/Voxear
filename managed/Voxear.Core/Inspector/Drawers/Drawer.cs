using System;
using Voxear.Interop;
using Voxear.Scripting;

namespace Voxear;

/// <summary>
/// Draws a field's value in the Inspector with Voxear UI, the UI games draw theirs with. Derive from ValueDrawer to
/// draw every field of a type, or from AttributeDrawer to draw the fields marked with an attribute of your own. The
/// field keeps its label and the attributes around it, so an InfoBox above a drawn field still shows. Drawers run
/// in edit mode; while the game plays, the field shows its own control.
/// <para>
/// Build the drawer's elements once, in Build, as Unity's UI Toolkit does: give them handlers that read and set
/// Value, Bind what they show, and add a NextField where the field's usual control goes. Or describe them in
/// Markup, with an element named "next" for it.
/// </para>
/// <code>
/// protected override void Build(UiElement root)
/// {
///     root.Add(new Label().Bind(() => $"{Value.Current} / {Value.Max}"));
///     root.Add(new Button("Fill", () => Value = Value with { Current = Value.Max }));
/// }
/// </code>
/// <para>
/// The elements are made once and kept, and only what changes is changed after, which is why there are two steps:
/// Build makes them, and Refresh, Bind or Draw bring them up to date when the value changes.
/// </para>
/// </summary>
public abstract class Drawer
{
	private readonly UiHost host = new UiHost();
	private UiElement? root;
	private string? shown;

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

	/// <summary>The type of the field's value, to tell whether a value changed as a scene writes it.</summary>
	internal Type ValueType { get; set; } = typeof(object);

	/// <summary>The markup the editor builds, an empty document when the drawer builds in code.</summary>
	internal string MarkupToBuild => Markup.Length > 0 ? Markup : "<vui></vui>";

	/// <summary>
	/// Adds the elements the value is drawn with to `root`, once, before the first draw, when Value already holds
	/// the field's value. Keep them in fields, or give them handlers and bindings as they are made. A NextField
	/// among them is where the field's next drawer or usual control goes; without one, the drawer draws it alone.
	/// </summary>
	protected virtual void Build(UiElement root)
	{
	}

	internal virtual void Attach(Attribute attribute)
	{
	}

	/// <summary>
	/// Builds it the first time, and raises what was clicked or changed in what it built, then draws, refreshes it
	/// when the value is not the one its elements show, and works out what is bound again. Gives back the value as
	/// the handlers and Draw leave it.
	/// </summary>
	internal object? Draw(object? value, UiDocument ui)
	{
		var before = host.Enter();
		try
		{
			Take(value);
			if (root == null)
			{
				root = Native.IsConnected ? ui.Root : new Panel();
				Build(root);
			}
			else
			{
				root.DispatchTree();
			}
			var drawn = DrawValue(ui);
			RefreshWhenChanged(drawn);
			host.Refresh();
			return drawn;
		}
		finally
		{
			UiHost.Leave(before);
		}
	}

	/// <summary>Takes the value to draw, as the field has it now, for the handlers to read and change.</summary>
	internal abstract void Take(object? value);

	/// <summary>Runs Draw, the advanced path, and gives back the value as it and the handlers leave it.</summary>
	internal abstract object? DrawValue(UiDocument ui);

	/// <summary>Runs Refresh with the value.</summary>
	internal abstract void RefreshValue(object? value);

	/// <summary>The value as a scene writes it, to tell whether it changed.</summary>
	internal virtual string Written(object? value)
	{
		return InspectorDrawers.Written(value, ValueType);
	}

	private void RefreshWhenChanged(object? drawn)
	{
		var now = Written(drawn);
		if (now == shown)
		{
			return;
		}

		shown = now;
		RefreshValue(drawn);
	}
}
