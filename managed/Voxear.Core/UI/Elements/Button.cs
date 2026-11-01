namespace Voxear
{
	/// <summary>A button with text on it. Listen to Clicked, or look at WasClicked each frame.</summary>
	public class Button : UiElement
	{
		public Button(string text = "") : base(UiKind.Button, text)
		{
		}
	}
}
