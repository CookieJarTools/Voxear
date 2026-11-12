using System;
using System.Numerics;
using Voxear.Interop;

namespace Voxear.Scripting;

/// <summary>
/// Tells scripts of particles' strikes: the enabled behaviours on each system's Obj as OnParticleHit, then whatever
/// listens to Particles.Hit.
/// </summary>
internal static class ParticleMessages
{
	public static void Dispatch(ReadOnlySpan<ParticleHitData> hits)
	{
		foreach (var told in hits)
		{
			var point = new Vector3(told.PointX, told.PointY, told.PointZ);
			var normal = new Vector3(told.NormalX, told.NormalY, told.NormalZ);
			var hit = new ParticleHit(new Obj(told.Obj), (int)told.Emitter, point, normal, told.Speed);
			Tell(told.Obj, hit);
			Particles.Raise(hit);
		}
	}

	private static void Tell(ulong obj, ParticleHit hit)
	{
		foreach (var handle in Native.Behaviours(obj))
		{
			if (BehaviourTable.Get(handle) is { IsDestroyed: false, Enabled: true } behaviour)
			{
				Run(behaviour, hit);
			}
		}
	}

	private static void Run(Behaviour behaviour, ParticleHit hit)
	{
		try
		{
			behaviour.HitByParticle(hit);
		}
		catch (Exception exception)
		{
			Log.Exception(exception, behaviour);
		}
	}
}
