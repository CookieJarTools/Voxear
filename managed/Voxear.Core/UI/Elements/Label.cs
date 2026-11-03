namespace Voxear;

/// <summary>A line or more of text.</summary>
public class Label : UiElement
{
	public Label(string text = "") : base(UiKind.Label, text)
	{
	}

	/// <summary>A key's text in the game's language, following it as it changes: <c>new Label(Strings.Menu.Title)</c>.</summary>
	public Label(LocalizedString text) : base(UiKind.Label, text)
	{
	}
}
