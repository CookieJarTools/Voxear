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
		smoke.Collision = ParticleCollision.Depth;
		smoke.Forces = 1f;
		Check.True(smoke.Collision == ParticleCollision.Off, "nothing to collide");
		Check.Near(0f, smoke.Bounce + smoke.Friction + smoke.LifeLost + smoke.Forces);
		smoke.Trails = ParticleTrails.Ribbon;
		smoke.TrailTime = 1f;
		Check.True(smoke.Trails == ParticleTrails.Off, "nothing to leave a trail");
		Check.Near(0f, smoke.TrailTime + smoke.TrailWidth);
	}

	[Test]
	private static void AnObjWithoutAForceFieldReadsAsNoneAndABlastWithoutTheEngineDoesNothing()
	{
		var field = Obj.None.ForceField;
		Check.True(!field.IsValid && field == ForceField.None, "no Force Field on None");
		field.Kind = ForceKind.Vortex;
		field.Strength = 5f;
		field.Radius = 3f;
		Check.True(field.Kind == ForceKind.Attract, "nothing to change");
		Check.Near(0f, field.Strength + field.Radius);
		Check.True(Obj.None.AddForceField() == ForceField.None, "nothing to add it to");
		Particles.Push(Vector3.Zero, 4f, 10f);
	}
}
