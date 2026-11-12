using System;
using System.Linq;

namespace Voxear;

/// <summary>
/// Colours along the way from 0 to 1: up to eight keys of where and which colour, blended in straight lines, as a
/// Curve's numbers are. A Particle System's colour over a particle's life is a gradient; its opacity is a Curve of its
/// own, so a key's alpha is not used there.
/// <code>
/// // White hot, then orange, then dull red.
/// sparks.ColorGradient = new Gradient((0f, Color.White), (0.3f, Color.Hex("#ff8800")), (1f, Color.Hex("#601000")));
/// </code>
/// </summary>
public readonly struct Gradient : IEquatable<Gradient>
{
	public static readonly Gradient None = default;

	private readonly (float Time, Color Color)[]? keys;

	/// <summary>A gradient through keys, each where from 0 to 1 and which colour: put in order of where, the first eight.</summary>
	public Gradient(params ReadOnlySpan<(float Time, Color Color)> keys)
	{
		var kept = keys[..Math.Min(keys.Length, Curve.MaxKeys)].ToArray();
		for (var index = 0; index < kept.Length; index++)
		{
			kept[index].Time = Math.Clamp(kept[index].Time, 0f, 1f);
		}
		this.keys = [.. kept.OrderBy(key => key.Time)];
	}

	/// <summary>Its keys, in order of where.</summary>
	public ReadOnlySpan<(float Time, Color Color)> Keys => keys;

	public int Count => keys?.Length ?? 0;

	public bool IsEmpty => Count == 0;

	/// <summary>Its colour at a point from 0 to 1, or white with no keys.</summary>
	public Color Evaluate(float time)
	{
		if (keys == null || keys.Length == 0)
		{
			return Color.White;
		}

		var at = Math.Clamp(time, 0f, 1f);
		if (at <= keys[0].Time)
		{
			return keys[0].Color;
		}

		for (var index = 1; index < keys.Length; index++)
		{
			var (before, after) = (keys[index - 1], keys[index]);
			if (at <= after.Time)
			{
				var span = after.Time - before.Time;
				return span > 0f ? Color.Lerp(before.Color, after.Color, (at - before.Time) / span) : after.Color;
			}
		}
		return keys[^1].Color;
	}

	public static bool operator ==(Gradient left, Gradient right)
	{
		return left.Equals(right);
	}

	public static bool operator !=(Gradient left, Gradient right)
	{
		return !left.Equals(right);
	}

	public bool Equals(Gradient other)
	{
		return Keys.SequenceEqual(other.Keys);
	}

	public override bool Equals(object? other)
	{
		return other is Gradient gradient && Equals(gradient);
	}

	public override int GetHashCode()
	{
		var hash = new HashCode();
		foreach (var key in Keys)
		{
			hash.Add(key);
		}
		return hash.ToHashCode();
	}

	public override string ToString()
	{
		return IsEmpty ? "Gradient(none)" : $"Gradient({Count} keys)";
	}
}
