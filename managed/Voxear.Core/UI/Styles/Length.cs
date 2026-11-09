using System;
using System.Globalization;

namespace Voxear;

/// <summary>
/// A size or a place in an element's style: pixels, a percentage of the box it is in, or auto. A number is pixels,
/// so <c>bar.Style.Width = 120;</c> is 120 pixels wide, and <c>bar.Style.Width = Length.Percent(50);</c> half the box
/// it is in.
/// </summary>
public readonly struct Length : IEquatable<Length>
{
	public static readonly Length Auto = new Length(0f, LengthUnit.Auto);

	public Length(float value, LengthUnit unit = LengthUnit.Pixels)
	{
		Value = value;
		Unit = unit;
	}

	public float Value { get; }

	public LengthUnit Unit { get; }

	/// <summary>Pixels, as a plain number gives.</summary>
	public static Length Pixels(float value)
	{
		return new Length(value);
	}

	/// <summary>A percentage of the box it is in: 50 is half.</summary>
	public static Length Percent(float value)
	{
		return new Length(value, LengthUnit.Percent);
	}

	public static implicit operator Length(float pixels)
	{
		return new Length(pixels);
	}

	public static bool operator ==(Length left, Length right)
	{
		return left.Equals(right);
	}

	public static bool operator !=(Length left, Length right)
	{
		return !left.Equals(right);
	}

	public bool Equals(Length other)
	{
		return Unit == other.Unit && (Unit == LengthUnit.Auto || Value == other.Value);
	}

	public override bool Equals(object? other)
	{
		return other is Length length && Equals(length);
	}

	public override int GetHashCode()
	{
		return HashCode.Combine(Unit, Unit == LengthUnit.Auto ? 0f : Value);
	}

	/// <summary>As a style sheet writes it: "120px", "50%" or "auto".</summary>
	public override string ToString()
	{
		var number = Value.ToString(CultureInfo.InvariantCulture);
		return Unit switch
		{
			LengthUnit.Percent => $"{number}%",
			LengthUnit.Auto => "auto",
			_ => $"{number}px",
		};
	}
}
