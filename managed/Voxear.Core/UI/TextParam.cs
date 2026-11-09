using System;

namespace Voxear;

/// <summary>
/// One of a LocalizedLabel's values, filling {0}, {1} and on in its key's text: a value, a value worked out again each
/// frame, or an input action, whose controls show as the player's device marks them. A number, text or Money is a
/// value as it is: <c>new LocalizedLabel(Strings.Shop.Buy, TextParam.Action(confirm), price)</c>.
/// </summary>
public readonly struct TextParam
{
	private readonly object? value;
	private readonly Func<object?>? from;

	private TextParam(object? value, Func<object?>? from, InputAction? action)
	{
		this.value = value;
		this.from = from;
		InputAction = action;
	}

	/// <summary>Whether it is worked out again each frame.</summary>
	internal bool IsBound => from != null;

	/// <summary>The action whose controls it shows, or null for a value.</summary>
	internal InputAction? InputAction { get; }

	/// <summary>Its value now, for the key's text to write as its language does.</summary>
	internal object? Now => from != null ? from() : value;

	/// <summary>A value, written as the key's text says: as it is, or as <c>{1, number}</c> or <c>{1, currency}</c>.</summary>
	public static TextParam Value(object? value)
	{
		return new TextParam(value, null, null);
	}

	/// <summary>A value worked out again each frame, as money that changes: <c>TextParam.Bind(() => wallet.Coins)</c>.</summary>
	public static TextParam Bind(Func<object?> value)
	{
		return new TextParam(null, value, null);
	}

	/// <summary>
	/// An input action's controls, drawn as a prompt draws them: the key for the keyboard, the button as the
	/// player's pad marks it, following the device used last and rebinding.
	/// </summary>
	public static TextParam Action(InputAction action)
	{
		return new TextParam(null, null, action);
	}

	public static implicit operator TextParam(string value)
	{
		return Value(value);
	}

	public static implicit operator TextParam(int value)
	{
		return Value(value);
	}

	public static implicit operator TextParam(long value)
	{
		return Value(value);
	}

	public static implicit operator TextParam(float value)
	{
		return Value(value);
	}

	public static implicit operator TextParam(double value)
	{
		return Value(value);
	}

	public static implicit operator TextParam(decimal value)
	{
		return Value(value);
	}

	public static implicit operator TextParam(Money value)
	{
		return Value(value);
	}

	public static implicit operator TextParam(InputAction action)
	{
		return Action(action);
	}
}
