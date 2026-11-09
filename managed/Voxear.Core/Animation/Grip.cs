using System;

namespace Voxear;

/// <summary>
/// How far a hand's fingers curl at most, from 0 straight to 1 a fist, thumb first: what <see cref="RigArm.CloseFingers"/>
/// closes them toward, stopping each where it meets what the hand holds.
/// </summary>
public readonly struct Grip : IEquatable<Grip>
{
	/// <summary>Round a handle, the index finger out along it, as on a trigger.</summary>
	public static readonly Grip Trigger = new Grip(0.55f, 0.35f, 0.85f, 0.88f, 0.9f);

	/// <summary>Round something held from below.</summary>
	public static readonly Grip Support = new Grip(0.2f, 0.7f, 0.75f, 0.78f, 0.8f);

	/// <summary>Pinching something small.</summary>
	public static readonly Grip Pinch = new Grip(0.6f, 0.6f, 0.85f, 0.9f, 0.9f);

	/// <summary>Open, as when reaching.</summary>
	public static readonly Grip Open = new Grip(0.1f, 0.1f, 0.12f, 0.14f, 0.16f);

	public Grip(float thumb, float index, float middle, float ring, float pinky)
	{
		Thumb = thumb;
		Index = index;
		Middle = middle;
		Ring = ring;
		Pinky = pinky;
	}

	public float Thumb { get; }

	public float Index { get; }

	public float Middle { get; }

	public float Ring { get; }

	public float Pinky { get; }

	/// <summary>A finger's curl, thumb first.</summary>
	public float this[int finger] => finger switch
	{
		0 => Thumb,
		1 => Index,
		2 => Middle,
		3 => Ring,
		4 => Pinky,
		_ => throw new ArgumentOutOfRangeException(nameof(finger), "A hand has five fingers, thumb first."),
	};

	/// <summary><paramref name="t"/> of the way from one grip to another.</summary>
	public static Grip Lerp(Grip from, Grip to, float t)
	{
		static float Mix(float a, float b, float t) => a + (b - a) * t;
		return new Grip(Mix(from.Thumb, to.Thumb, t), Mix(from.Index, to.Index, t), Mix(from.Middle, to.Middle, t), Mix(from.Ring, to.Ring, t), Mix(from.Pinky, to.Pinky, t));
	}

	public static bool operator ==(Grip left, Grip right)
	{
		return left.Equals(right);
	}

	public static bool operator !=(Grip left, Grip right)
	{
		return !left.Equals(right);
	}

	public bool Equals(Grip other)
	{
		return Thumb == other.Thumb && Index == other.Index && Middle == other.Middle && Ring == other.Ring && Pinky == other.Pinky;
	}

	public override bool Equals(object? other)
	{
		return other is Grip grip && Equals(grip);
	}

	public override int GetHashCode()
	{
		return HashCode.Combine(Thumb, Index, Middle, Ring, Pinky);
	}

	public override string ToString()
	{
		return $"Grip({Thumb}, {Index}, {Middle}, {Ring}, {Pinky})";
	}
}
