using System;
using System.Collections.Generic;

namespace Voxear;

/// <summary>
/// A key's text that keeps itself up to date: Text is always the key's text in the game's language, with its values
/// filled in, and Changed is raised with the new text whenever it changes, because the language changed, a table was
/// edited while the game runs, or a value was set. Show it once and it stays right.
/// <code>
/// var coins = new Localized("hud.coins", ("count", 0));
/// coins.Changed += text => label.Text = text;
/// coins.Set("count", 12); // "12 coins", or "12 pièces" in French
/// </code>
/// Dispose it when it is no longer shown, so it stops being looked up.
/// </summary>
public sealed class Localized : IDisposable
{
	private static readonly List<WeakReference<Localized>> live = new List<WeakReference<Localized>>();
	private static ulong seen;

	private readonly List<(string Name, object? Value)> values;
	private string text;

	public Localized(LocalizedString key, params (string Name, object? Value)[] values)
	{
		Key = key;
		this.values = new List<(string Name, object? Value)>(values);
		text = Read();
		live.Add(new WeakReference<Localized>(this));
	}

	/// <summary>Raised with the new text each time it changes.</summary>
	public event Action<string>? Changed;

	/// <summary>The key it shows.</summary>
	public LocalizedString Key { get; }

	/// <summary>The key's text now, in the game's language, with its values.</summary>
	public string Text => text;

	/// <summary>Gives a value the text names, such as "count" in "{count, plural, …}", and shows the text again.</summary>
	public void Set(string name, object? value)
	{
		var at = values.FindIndex(pair => pair.Name == name);
		if (at >= 0)
		{
			values[at] = (name, value);
		}
		else
		{
			values.Add((name, value));
		}
		Refresh();
	}

	public void Dispose()
	{
		live.RemoveAll(reference => !reference.TryGetTarget(out var target) || target == this);
		Changed = null;
	}

	public override string ToString()
	{
		return text;
	}

	/// <summary>Looks every live one up again once the game's text or language has changed since the last frame.</summary>
	internal static void Dispatch()
	{
		var version = Interop.NativeText.Version();
		if (version == seen)
		{
			return;
		}
		seen = version;
		live.RemoveAll(reference => !reference.TryGetTarget(out _));
		foreach (var reference in live.ToArray())
		{
			if (reference.TryGetTarget(out var shown))
			{
				shown.Refresh();
			}
		}
	}

	private void Refresh()
	{
		var now = Read();
		if (now == text)
		{
			return;
		}
		text = now;
		Changed?.Invoke(now);
	}

	private string Read()
	{
		return values.Count == 0 ? Key.Value : Key.Format(values.ToArray());
	}
}
