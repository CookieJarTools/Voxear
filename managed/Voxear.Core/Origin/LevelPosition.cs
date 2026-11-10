using System;
using System.Numerics;

namespace Voxear;

/// <summary>
/// A position measured from the level's real middle, in doubles, so it is exact however far out it is, and the
/// world's middle moving (see Origin) does not change it: for keeping where something was across a long journey,
/// or saving it. Origin.FromLevel gives it back as a position in the world as it is now.
/// </summary>
public readonly record struct LevelPosition(double X, double Y, double Z)
{
	/// <summary>This position moved by an offset.</summary>
	public static LevelPosition operator +(LevelPosition at, Vector3 by)
	{
		return new LevelPosition(at.X + by.X, at.Y + by.Y, at.Z + by.Z);
	}

	/// <summary>How far and which way `from` is to `to`, as a Vector3.</summary>
	public static Vector3 operator -(LevelPosition to, LevelPosition from)
	{
		return new Vector3((float)(to.X - from.X), (float)(to.Y - from.Y), (float)(to.Z - from.Z));
	}

	/// <summary>How far apart two positions are, in metres.</summary>
	public static double Distance(LevelPosition a, LevelPosition b)
	{
		var (x, y, z) = (a.X - b.X, a.Y - b.Y, a.Z - b.Z);
		return Math.Sqrt(x * x + y * y + z * z);
	}
}
