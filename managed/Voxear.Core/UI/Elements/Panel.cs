namespace Voxear;

/// <summary>A box that lays out what is in it, in a column unless its style says otherwise: Voxear UI's Panel.</summary>
public class Panel : UiElement
{
	public Panel() : base(UiKind.Panel, string.Empty)
	{
	}

	/// <summary>A panel with these classes, as <c>new Panel("hud", "top")</c>.</summary>
	public Panel(params string[] classes) : this()
	{
		foreach (var name in classes)
		{
			AddClass(name);
		}
	}
}
