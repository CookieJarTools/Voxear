using System;

namespace Voxear;

/// <summary>
/// A key into the project's string tables, as Unity's LocalizedString: a behaviour field of this type shows in the
/// Inspector as a pick of the project's keys, and Value is the key's text in the language the game is in. A scene
/// writes it as its key.
/// <code>
/// public LocalizedString Title = "quest.intro.title";
///
/// protected override void Start()
/// {
///     Log.Info(Title.Value);
/// }
/// </code>
/// </summary>
public readonly struct LocalizedString : IEquatable<LocalizedString>
{
	public LocalizedString(string key)
	{
		Key = key;
	}

	/// <summary>The key, such as "menu.play"; empty for none.</summary>
	public string Key { get; } = string.Empty;

	/// <summary>Whether it names a key.</summary>
	public bool IsSet
	{
		get { return !string.IsNullOrEmpty(Key); }
	}

	/// <summary>The key's text in the game's language, or empty when it names none.</summary>
	public string Value
	{
		get { return IsSet ? Localization.Get(Key) : string.Empty; }
	}

	/// <summary>The key's text with the values it names filled in, as Localization.Format does.</summary>
	public string Format(params (string Name, object? Value)[] values)
	{
		return IsSet ? Localization.Format(Key, values) : string.Empty;
	}

	/// <summary>Its text, kept up to date as the language changes, with Changed raised each time: see <see cref="Localized"/>.</summary>
	public Localized Observe(params (string Name, object? Value)[] values)
	{
		return new Localized(this, values);
	}

	public static implicit operator LocalizedString(string key)
	{
		return new LocalizedString(key);
	}

	public static bool operator ==(LocalizedString left, LocalizedString right)
	{
		return left.Equals(right);
	}

	public static bool operator !=(LocalizedString left, LocalizedString right)
	{
		return !left.Equals(right);
	}

	public bool Equals(LocalizedString other)
	{
		return (Key ?? string.Empty) == (other.Key ?? string.Empty);
	}

	public override bool Equals(object? other)
	{
		return other is LocalizedString localized && Equals(localized);
	}

	public override int GetHashCode()
	{
		return (Key ?? string.Empty).GetHashCode(StringComparison.Ordinal);
	}

	/// <summary>The key's text, as Value.</summary>
	public override string ToString()
	{
		return Value;
	}
}
