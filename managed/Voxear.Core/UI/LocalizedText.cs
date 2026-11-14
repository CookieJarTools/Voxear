using System;
using System.Globalization;
using System.Numerics;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// An Obj's Localized Text: shows a string table key's text in an element of the Obj's UI Panel, text in the world,
/// or its Screen UI, in the game's language, and again whenever the language changes. Its key is picked in the
/// Inspector from the project's tables, and its element from the document's named ones; with none named it shows in
/// the first label. Values fill in what the key's text names, as {count}. Get it with Obj.LocalizedText, or give an
/// Obj one with Obj.AddLocalizedText.
/// <code>
/// var sign = Obj.LocalizedText;
/// sign.Key = Strings.Sign.Tavern;
/// sign.SetValue("owner", innkeeper.Name);
/// </code>
/// </summary>
public readonly struct LocalizedText : IEquatable<LocalizedText>, IObjComponent
{
	private const int KeyText = 3;
	private const int SetKeyText = 4;
	private const int ElementText = 5;
	private const int SetElementText = 6;
	private const int ValueText = 7;
	private const int SetValueText = 8;
	private const int ClearValuesValue = 9;
	private const int ShownText = 10;

	public static readonly LocalizedText None = default;

	internal LocalizedText(Obj obj)
	{
		Obj = obj;
	}

	/// <summary>The Obj it belongs to.</summary>
	public Obj Obj { get; }

	/// <summary>False when its Obj has no Localized Text, or is gone.</summary>
	public bool IsValid => Obj.IsValid && NativeComponent.Number(NativeComponent.LocalizedText, Obj, NativeComponent.Has, 0f) != 0f;

	/// <summary>The key whose text it shows.</summary>
	public LocalizedString Key
	{
		get { return new LocalizedString(NativeComponent.Text(NativeComponent.LocalizedText, Obj, KeyText) ?? string.Empty); }
		set { Tell(SetKeyText, value.Key); }
	}

	/// <summary>The name of the element of its document it shows in; empty for the first label.</summary>
	public string Element
	{
		get { return NativeComponent.Text(NativeComponent.LocalizedText, Obj, ElementText) ?? string.Empty; }
		set { Tell(SetElementText, value); }
	}

	/// <summary>Its text as it shows now, in the game's language, its values filled in.</summary>
	public string Text => NativeComponent.Text(NativeComponent.LocalizedText, Obj, ShownText) ?? string.Empty;

	/// <summary>A value its key's text names, by name, as written; empty when it has none.</summary>
	public string GetValue(string name)
	{
		return NativeComponent.Ask(NativeComponent.LocalizedText, Obj, ValueText, name) ?? string.Empty;
	}

	/// <summary>Fills in what its key's text names, as {count}: numbers as the language writes them, and plurals by them.</summary>
	public void SetValue(string name, object value)
	{
		var written = Convert.ToString(value, CultureInfo.InvariantCulture) ?? string.Empty;
		Tell(SetValueText, $"{name}\n{written}");
	}

	/// <summary>Takes a value out, leaving its name in the text as it is.</summary>
	public void RemoveValue(string name)
	{
		Tell(SetValueText, name);
	}

	/// <summary>Takes every value out.</summary>
	public void ClearValues()
	{
		NativeComponent.Set(NativeComponent.LocalizedText, Obj, ClearValuesValue, Vector4.Zero);
	}

	/// <summary>Takes the Localized Text off its Obj, leaving the element showing what it shows.</summary>
	public void Remove()
	{
		NativeComponent.Set(NativeComponent.LocalizedText, Obj, NativeComponent.Remove, Vector4.Zero);
	}

	public static bool operator ==(LocalizedText left, LocalizedText right)
	{
		return left.Obj == right.Obj;
	}

	public static bool operator !=(LocalizedText left, LocalizedText right)
	{
		return left.Obj != right.Obj;
	}

	public bool Equals(LocalizedText other)
	{
		return Obj == other.Obj;
	}

	public override bool Equals(object? other)
	{
		return other is LocalizedText text && Equals(text);
	}

	public override int GetHashCode()
	{
		return Obj.GetHashCode();
	}

	public override string ToString()
	{
		return $"{Obj}'s Localized Text";
	}

	private void Tell(int what, string text)
	{
		if (!NativeComponent.Tell(NativeComponent.LocalizedText, Obj, what, text) && Native.IsConnected)
		{
			Log.Error($"{Obj} has no Localized Text; give it one with AddLocalizedText.");
		}
	}
}
