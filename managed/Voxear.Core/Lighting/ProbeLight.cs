using System;
using System.Numerics;

namespace Voxear;

/// <summary>
/// The light reaching a point from every way, as the baked light probes round it give it: nine spherical
/// harmonics a colour (L2), already the light a surface facing each way takes. Ask for it with
/// BakedLighting.ProbeLightAt.
/// <code>
/// var light = BakedLighting.ProbeLightAt(player.Transform.Position);
/// var dark = light.Average.Length() &lt; 0.05f; // in the shadows: harder to see
/// </code>
/// </summary>
public readonly struct ProbeLight : IEquatable<ProbeLight>
{
	private readonly Vector3[]? harmonics;

	internal ProbeLight(Vector3[] harmonics)
	{
		this.harmonics = harmonics;
	}

	/// <summary>False where there are no baked probes to give light.</summary>
	public bool IsValid => harmonics is { Length: 9 };

	/// <summary>Each of the nine harmonics' red, green and blue, in the order the engine keeps them.</summary>
	public ReadOnlySpan<Vector3> Harmonics => harmonics ?? [];

	/// <summary>The light arriving from every way, averaged: what a surface facing any way takes, give or take.</summary>
	public Vector3 Average => IsValid ? harmonics![0] * 0.282095f : Vector3.Zero;

	/// <summary>The light a surface facing <paramref name="normal"/> takes, linear.</summary>
	public Vector3 Irradiance(Vector3 normal)
	{
		if (!IsValid)
		{
			return Vector3.Zero;
		}
		var d = normal.LengthSquared() > 0f ? Vector3.Normalize(normal) : Vector3.UnitY;
		ReadOnlySpan<float> y =
		[
			0.282095f,
			0.488603f * d.Y,
			0.488603f * d.Z,
			0.488603f * d.X,
			1.092548f * d.X * d.Y,
			1.092548f * d.Y * d.Z,
			0.315392f * (3f * d.Z * d.Z - 1f),
			1.092548f * d.X * d.Z,
			0.546274f * (d.X * d.X - d.Y * d.Y),
		];
		var sum = Vector3.Zero;
		for (var i = 0; i < 9; i++)
		{
			sum += harmonics![i] * y[i];
		}
		return Vector3.Max(sum, Vector3.Zero);
	}

	public bool Equals(ProbeLight other)
	{
		return Harmonics.SequenceEqual(other.Harmonics);
	}

	public override bool Equals(object? other)
	{
		return other is ProbeLight light && Equals(light);
	}

	public override int GetHashCode()
	{
		return IsValid ? harmonics![0].GetHashCode() : 0;
	}

	public override string ToString()
	{
		return IsValid ? $"probe light averaging {Average}" : "no probe light";
	}
}
