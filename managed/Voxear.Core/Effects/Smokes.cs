using System.Numerics;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// Shooting through smoke, blasting it away and asking whether it hides something. A shot cuts a tunnel along its
/// line through every smoke it passes through, which fills back in over a second or so; a blast clears a ball,
/// which fills back in over a few seconds.
/// <code>
/// // A gun's shot: through any smoke on the way to what it hit.
/// Smokes.Shoot(muzzle, hit.Point);
///
/// // An enemy sees the player only where the smoke lets it.
/// var seen = Smokes.SeeThrough(Obj.WorldPosition, player.WorldPosition) > 0.3f;
/// </code>
/// </summary>
public static unsafe class Smokes
{
	private const int ShootIt = 0;
	private const int BlastIt = 1;
	private const int SeeIt = 2;

	/// <summary>A shot from one point to another, cutting through the smoke on the way. Whether it met any.</summary>
	public static bool Shoot(Vector3 from, Vector3 to)
	{
		return Act(ShootIt, from, to, 0f) != 0f;
	}

	/// <summary>A blast clearing the smoke within a radius, in metres, of its centre.</summary>
	public static void Blast(Vector3 centre, float radius)
	{
		Act(BlastIt, centre, new Vector3(radius, 0f, 0f), 0f);
	}

	/// <summary>How much of what is at one point shows from another through the smoke between: 1 with none in the way, near 0 hidden.</summary>
	public static float SeeThrough(Vector3 from, Vector3 to)
	{
		return Act(SeeIt, from, to, 1f);
	}

	private static float Act(int what, Vector3 a, Vector3 b, float missing)
	{
		if (!Native.IsConnected)
		{
			return missing;
		}
		var values = stackalloc float[7] { a.X, a.Y, a.Z, b.X, b.Y, b.Z, 0f };
		return Native.Api->SmokeAct(what, values);
	}
}
