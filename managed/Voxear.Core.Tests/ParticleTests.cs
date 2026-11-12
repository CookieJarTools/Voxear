using System;
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
	private static void EffectsAreNamedByStarterOrPathAndPlayNothingWithoutTheEngine()
	{
		Check.True(ParticleEffect.Explosion.IsValid && ParticleEffect.Explosion.IsStarter, "a starter");
		Check.True(ParticleEffect.Load("builtin:muzzle_flash") == ParticleEffect.MuzzleFlash, "a starter by its name");
		Check.True(ParticleEffect.Fire != ParticleEffect.Smoke && ParticleEffect.None != ParticleEffect.Fire, "each its own");
		Check.True(!ParticleEffect.None.IsValid && !ParticleEffect.Load(string.Empty).IsValid, "none is none");
		var blast = ParticleEffect.Load("Effects/Blast.particles");
		Check.True(blast.IsValid && !blast.IsStarter, "a project's effect, kept by its path away from the engine");
		Check.True(blast == ParticleEffect.Load("Effects/Blast.particles"), "the same path, the same effect");
		Check.True(Particles.Play(ParticleEffect.Explosion, Vector3.Zero) == ParticleSystem.None, "nothing plays");
		Check.True(Particles.Play(ParticleEffect.Sparks, Vector3.Zero, Vector3.UnitX) == ParticleSystem.None, "nor thrown out sideways");
		Check.True(Particles.Play(ParticleEffect.MuzzleFlash, Obj.None, 7) == ParticleSystem.None, "nor on no Obj");
		Check.True(blast.Play(Vector3.One) == ParticleSystem.None, "nor from the effect");
		var sparks = Obj.None.Particles;
		sparks.Effect = ParticleEffect.Sparks;
		Check.True(sparks.Effect == ParticleEffect.None, "no system to give an effect");
	}

	[Test]
	private static void CurvesAndGradientsKeepTheirKeysInOrderAndJoinThemInStraightLines()
	{
		var swell = new Curve((1f, 0f), (0f, 0.5f), (0.25f, 1f));
		Check.True(swell.Count == 3 && swell.Keys[0].Time == 0f && swell.Keys[2].Time == 1f, "in order of where");
		Check.Near(0.75f, swell.Evaluate(0.125f));
		Check.Near(0.5f, swell.Evaluate(-1f));
		Check.Near(0f, swell.Evaluate(2f));
		Check.Near(3f, Curve.None.Evaluate(0.5f, 3f));
		Check.True(swell == new Curve((0f, 0.5f), (0.25f, 1f), (1f, 0f)) && swell != Curve.Flat(1f), "alike by keys");
		var many = new Curve((0f, 0f), (0.1f, 0f), (0.2f, 0f), (0.3f, 0f), (0.4f, 0f), (0.5f, 0f), (0.6f, 0f), (0.7f, 0f), (0.8f, 0f));
		Check.True(many.Count == Curve.MaxKeys, "no more than eight keys");
		var hot = new Gradient((1f, Color.Black), (0f, Color.White));
		Check.True(hot.Evaluate(0.5f) == Color.Lerp(Color.White, Color.Black, 0.5f), "half way between");
		Check.True(Gradient.None.Evaluate(0.3f) == Color.White && Gradient.None.IsEmpty, "white with none");
	}

	[Test]
	private static void AnObjWithoutParticlesHasNoCurvesAndHearsOfNoHits()
	{
		var sparks = Obj.None.Particles;
		sparks.SizeCurve = Curve.Flat(2f);
		sparks.ColorGradient = new Gradient((0f, Color.Red));
		sparks.HitEvents = true;
		Check.True(sparks.SizeCurve.IsEmpty && sparks.OpacityCurve.IsEmpty && sparks.SpeedCurve.IsEmpty, "no curves");
		Check.True(sparks.ColorGradient.IsEmpty && !sparks.HitEvents, "nor a gradient, nor hit events");
		var heard = 0;
		Action<ParticleHit> listen = _ => heard++;
		Particles.Hit += listen;
		Particles.Hit -= listen;
		Check.True(heard == 0, "nothing struck");
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
