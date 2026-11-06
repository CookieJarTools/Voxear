using System;
using System.Globalization;

namespace Voxear;

/// <summary>
/// A colour as it looks on screen (sRGB), with alpha, each from 0 to 1, or past 1 for an HDR colour that glows. Debug
/// draw takes one, and it is plain data, so components can hold one too.
/// </summary>
public readonly struct Color : IEquatable<Color>
{
	public static readonly Color White = new Color(1f, 1f, 1f);
	public static readonly Color Black = new Color(0f, 0f, 0f);
	public static readonly Color Grey = new Color(0.5f, 0.5f, 0.5f);
	public static readonly Color Red = new Color(1f, 0f, 0f);
	public static readonly Color Green = new Color(0f, 1f, 0f);
	public static readonly Color Blue = new Color(0f, 0f, 1f);
	public static readonly Color Yellow = new Color(1f, 0.92f, 0.016f);
	public static readonly Color Cyan = new Color(0f, 1f, 1f);
	public static readonly Color Magenta = new Color(1f, 0f, 1f);
	public static readonly Color Clear = new Color(0f, 0f, 0f, 0f);

	public Color(float r, float g, float b, float a = 1f)
	{
		R = r;
		G = g;
		B = b;
		A = a;
	}

	public float R { get; }

	public float G { get; }

	public float B { get; }

	public float A { get; }

	/// <summary>A colour from hex, as a design tool writes it: "#ff8800", or "#ff880080" with alpha. The # is optional.</summary>
	public static Color Hex(string hex)
	{
		var digits = hex.TrimStart('#');
		if (digits.Length != 6 && digits.Length != 8 || !uint.TryParse(digits, NumberStyles.HexNumber, CultureInfo.InvariantCulture, out var value))
		{
			throw new FormatException($"\"{hex}\" is not a colour; write it as #rrggbb or #rrggbbaa");
		}
		if (digits.Length == 6)
		{
			value = value << 8 | 0xff;
		}
		return new Color((value >> 24) / 255f, (value >> 16 & 0xff) / 255f, (value >> 8 & 0xff) / 255f, (value & 0xff) / 255f);
	}

	/// <summary>From a to b, as t goes from 0 to 1, held within them.</summary>
	public static Color Lerp(Color a, Color b, float t)
	{
		t = Math.Clamp(t, 0f, 1f);
		return new Color(a.R + (b.R - a.R) * t, a.G + (b.G - a.G) * t, a.B + (b.B - a.B) * t, a.A + (b.A - a.A) * t);
	}

	/// <summary>
	/// How many stops brighter than white an HDR colour is, as its picker shows it: 0 for a colour within 0 to 1,
	/// 1 for one whose brightest channel is 2.
	/// </summary>
	public float Intensity => MathF.Max(MathF.Log2(Brightest), 0f);

	private float Brightest => MathF.Max(R, MathF.Max(G, B));

	public Color WithAlpha(float alpha)
	{
		return new Color(R, G, B, alpha);
	}

	/// <summary>
	/// The same colour at this intensity, in stops: Color.Red.WithIntensity(2) is four times as bright as red, for an
	/// HDR field to glow. 0 brings an HDR colour back within 0 to 1.
	/// </summary>
	public Color WithIntensity(float stops)
	{
		var gain = MathF.Pow(2f, stops) / MathF.Max(Brightest, 1f);
		return new Color(R * gain, G * gain, B * gain, A);
	}

	public static bool operator ==(Color left, Color right)
	{
		return left.Equals(right);
	}

	public static bool operator !=(Color left, Color right)
	{
		return !left.Equals(right);
	}

	public bool Equals(Color other)
	{
		return R == other.R && G == other.G && B == other.B && A == other.A;
	}

	public override bool Equals(object? other)
	{
		return other is Color color && Equals(color);
	}

	public override int GetHashCode()
	{
		return HashCode.Combine(R, G, B, A);
	}

	public override string ToString()
	{
		return string.Create(CultureInfo.InvariantCulture, $"Color({R:0.###}, {G:0.###}, {B:0.###}, {A:0.###})");
	}
}
