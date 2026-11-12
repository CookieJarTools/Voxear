using System.Numerics;

namespace Voxear.Tests;

/// <summary>The Particle System API with no engine behind it: no Obj has one, and nothing throws.</summary>
internal static class ParticleTests
{
	[Test]
	private static void AnObjWithoutParticlesReadsAsNothingPlaying()
	{
		var sparks = Obj.None.Particles;
		Check.True(!sparks.IsValid && sparks == ParticleSystem.None, "no Particle System on None");
		Check.True(!sparks.IsPlaying && sparks.Color == Color.White && sparks.Lifetime == Vector2.Zero, "nothing out");
		sparks.Rate = 40f;
		sparks.Play();
		sparks.Stop();
		Check.Near(0f, sparks.Rate);
		Check.True(Obj.None.AddParticles() == ParticleSystem.None, "nothing to add it to");
	}

	[Test]
	private static void AnObjWithoutParticlesIsNotSimulatedAndKeepsNoWind()
	{
		var smoke = Obj.None.Particles;
		smoke.Space = ParticleSpace.World;
		smoke.Wind = 1f;
		smoke.Turbulence = 0.5f;
		smoke.TurbulenceSize = 2f;
		Check.True(!smoke.IsSimulated && smoke.Space == ParticleSpace.Local, "nothing to simulate");
		Check.Near(0f, smoke.Wind + smoke.Turbulence + smoke.TurbulenceSize);
	}
}
