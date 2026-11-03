using System;

namespace Voxear;

/// <summary>
/// A button with text on it. Give it what to run when it is clicked, as <c>new Button("Fill", Fill)</c> or
/// <c>new Button("Fill").OnClicked(Fill)</c>; or listen to Clicked, or look at WasClicked each frame.
/// </summary>
public class Button : UiElement
{
	public Button(string text = "") : base(UiKind.Button, text)
	{
	}

	/// <summary>A button that runs `clicked` each time it is clicked, as Unity's <c>new Button(action)</c>.</summary>
	public Button(string text, Action clicked) : this(text)
	{
		Clicked += clicked;
	}

	/// <summary>A button with a key's text in the game's language, following it as it changes.</summary>
	public Button(LocalizedString text) : base(UiKind.Button, text)
	{
	}

	public Button(LocalizedString text, Action clicked) : this(text)
	{
		Clicked += clicked;
	}
}
