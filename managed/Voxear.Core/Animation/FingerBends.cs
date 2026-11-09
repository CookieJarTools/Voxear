using System;

namespace Voxear;

/// <summary>
/// How far each of a hand's fingers' first three joints is bent toward the palm, in radians, thumb first: a hand's
/// shape. <see cref="Of(Grip)"/> gives a grip's most, and <see cref="RigArm.Bends"/> a posed hand's own.
/// </summary>
public readonly struct FingerBends : IEquatable<FingerBends>
{
	public const int Count = 15;

	// How far a finger's joints bend at a full curl, from its root out, in degrees, and the thumb's.
	private static readonly float[] FingerBend = { 75f, 95f, 60f };
	private static readonly float[] ThumbBend = { 25f, 35f, 40f };

	private readonly float[]? joints;

	internal FingerBends(float[] joints)
	{
		this.joints = joints;
	}

	/// <summary>Every finger straight.</summary>
	public static FingerBends Straight => default;

	/// <summary>A joint's bend: finger 0 is the thumb, joint 0 the knuckle.</summary>
	public float this[int finger, int joint] => joints?[finger * 3 + joint] ?? 0f;

	/// <summary>The most a grip lets each joint bend.</summary>
	public static FingerBends Of(Grip grip)
	{
		var bends = new float[Count];
		for (var finger = 0; finger < 5; finger++)
		{
			var most = finger == 0 ? ThumbBend : FingerBend;
			for (var joint = 0; joint < 3; joint++)
			{
				bends[finger * 3 + joint] = most[joint] * Math.Clamp(grip[finger], 0f, 1f) * MathF.PI / 180f;
			}
		}
		return new FingerBends(bends);
	}

	/// <summary><paramref name="t"/> of the way from one shape to another.</summary>
	public static FingerBends Lerp(FingerBends from, FingerBends to, float t)
	{
		var bends = new float[Count];
		for (var i = 0; i < Count; i++)
		{
			var a = from.joints?[i] ?? 0f;
			var b = to.joints?[i] ?? 0f;
			bends[i] = a + (b - a) * t;
		}
		return new FingerBends(bends);
	}

	internal void CopyTo(Span<float> to)
	{
		for (var i = 0; i < Count; i++)
		{
			to[i] = joints?[i] ?? 0f;
		}
	}

	public static bool operator ==(FingerBends left, FingerBends right)
	{
		return left.Equals(right);
	}

	public static bool operator !=(FingerBends left, FingerBends right)
	{
		return !left.Equals(right);
	}

	public bool Equals(FingerBends other)
	{
		for (var i = 0; i < Count; i++)
		{
			if ((joints?[i] ?? 0f) != (other.joints?[i] ?? 0f))
			{
				return false;
			}
		}
		return true;
	}

	public override bool Equals(object? other)
	{
		return other is FingerBends bends && Equals(bends);
	}

	public override int GetHashCode()
	{
		var hash = new HashCode();
		for (var i = 0; i < Count; i++)
		{
			hash.Add(joints?[i] ?? 0f);
		}
		return hash.ToHashCode();
	}
}
