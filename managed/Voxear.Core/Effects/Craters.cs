using System.Numerics;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// Blasting craters into the ground. A crater digs into the level's Crater Field, and shows on every surface there
/// whose material takes craters, where it faces up. Craters last until the level ends, or Clear fills them in.
/// <code>
/// // A shell landing: a crater, and a hole blown through any smoke.
/// Craters.Blast(hit.Point, 2.5f, 0.8f);
/// Smokes.Blast(hit.Point, 4f);
/// </code>
/// </summary>
public static unsafe class Craters
{
	private const int BlastIt = 0;
	private const int ClearAll = 1;

	/// <summary>A crater at a point, its radius and depth in metres. False when the level has no Crater Field.</summary>
	public static bool Blast(Vector3 centre, float radius, float depth)
	{
		return Act(BlastIt, centre, radius, depth) != 0f;
	}

	/// <summary>Fills every crater back in.</summary>
	public static void Clear()
	{
		Act(ClearAll, Vector3.Zero, 0f, 0f);
	}

	private static float Act(int what, Vector3 centre, float radius, float depth)
	{
		if (!Native.IsConnected)
		{
			return 0f;
		}
		var values = stackalloc float[7] { centre.X, centre.Y, centre.Z, radius, depth, 0f, 0f };
		return Native.Api->CratersAct(what, values);
	}
}
