using System;
using System.Numerics;
using Voxear.Interop;

namespace Voxear.Scripting;

/// <summary>
/// Tells behaviours of a physics step's contacts and trigger crossings: OnCollisionEnter, OnCollisionExit,
/// OnTriggerEnter and OnTriggerExit. The engine sends each event once for each Obj in it, and every behaviour on that
/// Obj is told, enabled or not, as in Unity.
/// </summary>
internal static class PhysicsMessages
{
	public static void Dispatch(ReadOnlySpan<PhysicsEventData> events)
	{
		foreach (var told in events)
		{
			Tell(told);
		}
	}

	private static void Tell(in PhysicsEventData told)
	{
		var point = new Vector3(told.PointX, told.PointY, told.PointZ);
		var normal = new Vector3(told.NormalX, told.NormalY, told.NormalZ);
		var collision = new Collision(new Obj(told.B), point, normal);
		foreach (var handle in Native.Behaviours(told.A))
		{
			if (BehaviourTable.Get(handle) is { IsDestroyed: false } behaviour)
			{
				Run(behaviour, told.Kind, collision);
			}
		}
	}

	private static void Run(Behaviour behaviour, uint kind, Collision collision)
	{
		try
		{
			behaviour.Collide(kind, collision);
		}
		catch (Exception exception)
		{
			Log.Exception(exception, behaviour);
		}
	}
}
