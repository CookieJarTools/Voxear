using System;
using System.Linq;

namespace Voxear;

/// <summary>
/// Numbers along the way from 0 to 1: up to eight keys of where and what, joined by straight lines, flat before the
/// first and after the last. A Particle System's size, opacity and speed over a particle's life are curves. With no
/// keys it has no value, and what uses it does as it would without one.
/// <code>
/// // Grows quickly, then shrinks away.
/// smoke.SizeCurve = new Curve((0f, 0.5f), (0.2f, 1f), (1f, 0f));
/// </code>
/// </summary>
public readonly struct Curve : IEquatable<Curve>
{
	/// <summary>The most keys a curve holds.</summary>
	public const int MaxKeys = 8;

	public static readonly Curve None = default;

	private readonly (float Time, float Value)[]? keys;

	/// <summary>A curve through keys, each where from 0 to 1 and what: put in order of where, the first eight.</summary>
	public Curve(params ReadOnlySpan<(float Time, float Value)> keys)
	{
		var kept = keys[..Math.Min(keys.Length, MaxKeys)].ToArray();
		for (var index = 0; index < kept.Length; index++)
		{
			kept[index].Time = Math.Clamp(kept[index].Time, 0f, 1f);
		}
		this.keys = [.. kept.OrderBy(key => key.Time)];
	}

	/// <summary>Its keys, in order of where.</summary>
	public ReadOnlySpan<(float Time, float Value)> Keys => keys;

	public int Count => keys?.Length ?? 0;

	public bool IsEmpty => Count == 0;

	/// <summary>The same all along.</summary>
	public static Curve Flat(float value)
	{
		return new Curve((0f, value), (1f, value));
	}

	/// <summary>Its value at a point from 0 to 1, or <paramref name="otherwise"/> with no keys.</summary>
	public float Evaluate(float time, float otherwise = 1f)
	{
		if (keys == null || keys.Length == 0)
		{
			return otherwise;
		}

		var at = Math.Clamp(time, 0f, 1f);
		if (at <= keys[0].Time)
		{
			return keys[0].Value;
		}

		for (var index = 1; index < keys.Length; index++)
		{
			var (before, after) = (keys[index - 1], keys[index]);
			if (at <= after.Time)
			{
				var span = after.Time - before.Time;
				return span > 0f ? before.Value + (after.Value - before.Value) * (at - before.Time) / span : after.Value;
			}
		}
		return keys[^1].Value;
	}

	public static bool operator ==(Curve left, Curve right)
	{
		return left.Equals(right);
	}

	public static bool operator !=(Curve left, Curve right)
	{
		return !left.Equals(right);
	}

	public bool Equals(Curve other)
	{
		return Keys.SequenceEqual(other.Keys);
	}

	public override bool Equals(object? other)
	{
		return other is Curve curve && Equals(curve);
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
		return IsEmpty ? "Curve(none)" : $"Curve({string.Join(", ", keys!.Select(key => $"{key.Time:0.##}: {key.Value:0.###}"))})";
	}
}
