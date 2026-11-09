using System;
using System.Numerics;

namespace Voxear;

/// <summary>
/// A box in the world, turned and scaled as its frame says, with its corners in its own space: what a hand's palm is
/// put against and its fingers close round, as the parts of something held.
/// </summary>
public readonly struct OrientedBox : IEquatable<OrientedBox>
{
	public OrientedBox(Matrix4x4 frame, Vector3 least, Vector3 most)
	{
		Frame = frame;
		Least = least;
		Most = most;
	}

	/// <summary>Where the box is in the world, and how it is turned and scaled.</summary>
	public Matrix4x4 Frame { get; }

	/// <summary>Its lowest corner, in its own space.</summary>
	public Vector3 Least { get; }

	/// <summary>Its highest corner, in its own space.</summary>
	public Vector3 Most { get; }

	/// <summary>The box carried by another frame, as a part's box moves with the part.</summary>
	public OrientedBox MovedBy(Matrix4x4 frame)
	{
		return new OrientedBox(Frame * frame, Least, Most);
	}

	internal void CopyTo(Span<float> to)
	{
		var frame = Frame;
		for (var row = 0; row < 4; row++)
		{
			for (var column = 0; column < 4; column++)
			{
				to[row * 4 + column] = frame[row, column];
			}
		}
		to[16] = Least.X;
		to[17] = Least.Y;
		to[18] = Least.Z;
		to[19] = Most.X;
		to[20] = Most.Y;
		to[21] = Most.Z;
	}

	public static bool operator ==(OrientedBox left, OrientedBox right)
	{
		return left.Equals(right);
	}

	public static bool operator !=(OrientedBox left, OrientedBox right)
	{
		return !left.Equals(right);
	}

	public bool Equals(OrientedBox other)
	{
		return Frame == other.Frame && Least == other.Least && Most == other.Most;
	}

	public override bool Equals(object? other)
	{
		return other is OrientedBox box && Equals(box);
	}

	public override int GetHashCode()
	{
		return HashCode.Combine(Frame, Least, Most);
	}
}
