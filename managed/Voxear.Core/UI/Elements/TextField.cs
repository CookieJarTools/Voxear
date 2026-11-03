using System;

namespace Voxear;

/// <summary>A line to type into. Its Text is what was typed.</summary>
public class TextField : UiElement
{
	private const int StringPlaceholder = 1;

	private UiText placeholder;
	private Action<string>? typed;

	public TextField(string text = "") : base(UiKind.TextField, text)
	{
	}

	/// <summary>Runs with the text each time it is typed in.</summary>
	public event Action<string>? Typed
	{
		add
		{
			typed += value;
			Listen();
		}
		remove
		{
			typed -= value;
		}
	}

	/// <summary>
	/// What shows, faded, while nothing is typed: written out, as "Your name", or a key's text, as
	/// Strings.Menu.NameHint, which follows the game's language.
	/// </summary>
	public UiText Placeholder
	{
		get { return placeholder; }
		set
		{
			placeholder = value;
			SendString(StringPlaceholder, value.ToString());
			Track();
		}
	}

	/// <summary>Runs `handler` with the text each time it is typed in, and gives the field back.</summary>
	public TextField OnTyped(Action<string> handler)
	{
		Typed += handler;
		return this;
	}

	private protected override bool KindIsBound => placeholder.IsLocalized;

	private protected override void OnShown()
	{
		if (!placeholder.IsEmpty)
		{
			SendString(StringPlaceholder, placeholder.ToString());
		}
	}

	private protected override void RefreshKind(bool languageChanged)
	{
		if (languageChanged && placeholder.IsLocalized)
		{
			SendString(StringPlaceholder, placeholder.ToString());
		}
	}

	private protected override bool KindListens => typed != null;

	private protected override void RaiseKindChanged()
	{
		typed?.Invoke(Text);
	}
}
