namespace Voxear;

/// <summary>A line or more of text.</summary>
public class Label : UiElement
{
	public Label(string text = "") : base(UiKind.Label, text)
	{
	}
}
