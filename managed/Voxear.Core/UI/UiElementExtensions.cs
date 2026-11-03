using System;

namespace Voxear;

/// <summary>
/// What every element can be given as it is made, each giving the element back so it chains inside Add:
/// <code>
/// root.Add(new Button("Fill").OnClicked(() => Value = Value with { Current = Value.Max }));
/// root.Add(new Label().Bind(() => $"{Value.Current} / {Value.Max}"));
/// root.Add(new Label().Localize(Strings.Hud.Coins, ("count", coins)));
/// </code>
/// <para>
/// Bind takes a function, not a value, because the element shows what the function gives each time it is drawn:
/// once a frame in a game, and in a drawer or window each time it draws. A value would be read once and stay as it
/// was. For a slider, toggle or dropdown, its own Bind sets its value the same way.
/// </para>
/// </summary>
public static class UiElementExtensions
{
	extension<T>(T element) where T : UiElement
	{
		/// <summary>Runs `clicked` each time it is clicked, at the start of the next frame or as a drawer draws.</summary>
		public T OnClicked(Action clicked)
		{
			element.Clicked += clicked;
			return element;
		}

		/// <summary>Runs `changed` each time its value changes on screen: typed in, ticked, moved or picked.</summary>
		public T OnChanged(Action changed)
		{
			element.Changed += changed;
			return element;
		}

		/// <summary>
		/// Shows what `text` gives as its text, worked out again each frame, or each time its drawer or window
		/// draws: <c>label.Bind(() => $"{Value.Current} / {Value.Max}")</c>. Setting Text stops it.
		/// </summary>
		public T Bind(Func<string> text)
		{
			element.BindText(text, eachPass: true);
			return element;
		}

		/// <summary>Shows the text of the key `key` gives, which can change: <c>() => Alive ? Strings.Hud.Alive : Strings.Hud.Down</c>.</summary>
		public T Bind(Func<LocalizedString> key)
		{
			element.BindText(() => key().Value, eachPass: true);
			return element;
		}

		/// <summary>Shows a key's text in the game's language, following it as it changes, as Localize does.</summary>
		public T Bind(LocalizedString key)
		{
			element.Follow(new UiText(key));
			return element;
		}

		/// <summary>Shows a Localized text, following its language and values as they change.</summary>
		public T Bind(Localized text)
		{
			element.BindText(() => text.Text, eachPass: true);
			return element;
		}
	}

	/// <summary>
	/// Shows a key's text in the game's language with the values it names filled in, following the language as it
	/// changes: <c>label.Localize(Strings.Hud.Coins, ("count", 3))</c>. Call it again for other values.
	/// </summary>
	// Written the older way: in an extension block, its tuple values are warned of as wrongly nullable.
	public static T Localize<T>(this T element, LocalizedString key, params (string Name, object? Value)[] values)
		where T : UiElement
	{
		element.Follow(new UiText(key, values));
		return element;
	}
}
