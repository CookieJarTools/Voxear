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

	[Test]
	private static void AnObjWithoutSmokeReadsAsNotPlaying()
	{
		var smoke = Obj.None.Smoke;
		Check.True(!smoke.IsValid && smoke == Smoke.None, "no Smoke on None");
		Check.True(!smoke.IsPlaying && !smoke.PlayOnStart && smoke.Color == Color.White, "nothing there");
		smoke.Size = 6f;
		smoke.Play();
		smoke.Disperse();
		Check.Near(0f, smoke.Size);
		Check.True(Obj.None.AddSmoke() == Smoke.None, "nothing to add it to");
	}
}
