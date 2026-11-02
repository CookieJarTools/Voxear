using System;
using System.Collections.Generic;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// The game's text in each of its languages, as Unity's Localization package keeps it. Get finds a key's text in
/// the language the game is in, or the nearest language that has it, as "pt" for "pt-BR", and at last the language
/// the project is written in. The project lists its languages in voxear.toml, and its string tables, .strings
/// files, hold the text.
/// <para>
/// A key no language has reads as ⟦key⟧ while the game is made, with a warning in the Console, and as the key itself
/// in a shipped game.
/// </para>
/// <code>
/// title.Text = Localization.Get("menu.play");
/// var found = Localization.Format("loot.found", ("player", name), ("count", coins));
/// Localization.Language = "pt-BR";
/// </code>
/// </summary>
public static class Localization
{
	/// <summary>
	/// The language the game is in, by its code, such as "en" or "pt-BR". The game starts in the player's system
	/// language when the project has it, or the nearest it has. Set to a language the project lacks, it takes the
	/// nearest the project has, or stays as it is when none is near.
	/// </summary>
	public static string Language
	{
		get { return NativeText.Language(null); }
		set
		{
			var before = NativeText.Language(null);
			if (NativeText.Language(value) != before)
			{
				Changed?.Invoke();
			}
		}
	}

	/// <summary>The languages the project has, the one it is written in first.</summary>
	public static IReadOnlyList<string> Languages
	{
		get { return NativeText.Languages(); }
	}

	/// <summary>Raised when the game's Language changes, for text shown from it to be looked up again.</summary>
	public static event Action? Changed;

	/// <summary>A key's text in the game's language.</summary>
	public static string Get(string key)
	{
		return NativeText.Get(key, null);
	}

	/// <summary>
	/// A key's text with the values it names filled in: "{player} found {count} coins" takes the values named player
	/// and count. As in ICU's MessageFormat, "{count, plural, one {# coin} other {# coins}}" picks its words by the
	/// language's plural rules, "{g, select, female {her} other {their}}" by the value, and "{count, number}" writes a
	/// number as the language does. A name given no value is left as it is.
	/// </summary>
	public static string Format(string key, params (string Name, object? Value)[] values)
	{
		return NativeText.Get(key, values);
	}
}
