using System;

namespace Voxear;

/// <summary>
/// Text an element shows: written out, as "Fills the bar", or a key of the game's string tables, as
/// Strings.Hints.Fill, in the game's language and following it when it changes. Either goes wherever an element
/// takes a UiText, such as its Tooltip or a text field's Placeholder:
/// <code>
/// root.Add(new Button("Fill") { Tooltip = Strings.Hints.Fill });
/// root.Add(new TextField { Placeholder = "Your name" });
/// root.Add(new Label { Tooltip = new UiText(Strings.Hud.Coins, ("count", 12)) });
/// </code>
/// </summary>
public readonly struct UiText
{
	private readonly string? written;
	private readonly (string Name, object? Value)[]? values;

	public UiText(string text)
	{
		written = text;
	}

	/// <summary>A key's text, with the values it names filled in, as "{count} coins" with ("count", 12).</summary>
	public UiText(LocalizedString key, params (string Name, object? Value)[] values)
	{
		Key = key;
		this.values = values;
	}

	/// <summary>The key it shows, or none for text written out.</summary>
	public LocalizedString Key { get; }

	/// <summary>Whether it is a key's text, which follows the game's language.</summary>
	public bool IsLocalized => Key.IsSet;

	/// <summary>Whether it shows nothing.</summary>
	public bool IsEmpty => !Key.IsSet && string.IsNullOrEmpty(written);

	/// <summary>The values the key's text names.</summary>
	public ReadOnlySpan<(string Name, object? Value)> Values => values ?? [];

	public static implicit operator UiText(string text)
	{
		return new UiText(text);
	}

	public static implicit operator UiText(LocalizedString key)
	{
		return new UiText(key);
	}

	/// <summary>What it shows now: the text written out, or the key's text in the game's language.</summary>
	public override string ToString()
	{
		if (!Key.IsSet)
		{
			return written ?? string.Empty;
		}
		return values is { Length: > 0 } ? Key.Format(values) : Key.Value;
	}
}
