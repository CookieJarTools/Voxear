using System;
using System.Numerics;

namespace Voxear.Tests;

/// <summary>Assertions that throw with a message saying what was expected.</summary>
internal static class Check
{
	private const float Tolerance = 1e-4f;

	public static void True(bool condition, string what)
	{
		if (!condition)
		{
			throw new Exception(what);
		}
	}

	public static void Near(float expected, float actual, float tolerance = Tolerance)
	{
		True(MathF.Abs(expected - actual) <= tolerance, $"expected {expected}, got {actual}");
	}

	public static void Near(Vector3 expected, Vector3 actual, float tolerance = Tolerance)
	{
		True(Vector3.Distance(expected, actual) <= tolerance, $"expected {expected}, got {actual}");
	}

	public static void Near(Vector2 expected, Vector2 actual, float tolerance = Tolerance)
	{
		True(Vector2.Distance(expected, actual) <= tolerance, $"expected {expected}, got {actual}");
	}

	/// <summary>Two rotations that turn things the same way; q and -q are the same rotation.</summary>
	public static void Same(Quaternion expected, Quaternion actual)
	{
		True(MathF.Abs(Quaternion.Dot(expected, actual)) >= 1f - Tolerance, $"expected {expected}, got {actual}");
	}
}
