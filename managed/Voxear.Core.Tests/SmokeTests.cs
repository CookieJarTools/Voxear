using System.Numerics;

namespace Voxear.Tests;

/// <summary>Smoke with no engine behind it: nothing in the way, and nothing throws.</summary>
internal static class SmokeTests
{
	[Test]
	private static void WithNoSmokeEverythingShowsAndShotsMeetNone()
	{
		Check.Near(1f, Smokes.SeeThrough(Vector3.Zero, Vector3.UnitX * 10f));
		Check.True(!Smokes.Shoot(Vector3.Zero, Vector3.UnitX), "no smoke to shoot through");
		Smokes.Blast(Vector3.Zero, 3f);
	}
}
