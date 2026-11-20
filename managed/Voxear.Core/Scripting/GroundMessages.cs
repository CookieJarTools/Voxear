using System;
using Voxear.Interop;

namespace Voxear.Scripting;

/// <summary>Tells behaviours their Obj's ground goes, as OnGroundUnloading, or came back, as OnGroundLoaded.</summary>
internal static class GroundMessages
{
	public static void Dispatch(ReadOnlySpan<ulong> objs, ReadOnlySpan<int> kinds)
	{
		for (var index = 0; index < objs.Length; index++)
		{
			Tell(objs[index], kinds[index] == 0);
		}
	}

	private static void Tell(ulong obj, bool going)
	{
		foreach (var handle in Native.Behaviours(obj))
		{
			if (BehaviourTable.Get(handle) is not { IsDestroyed: false, Enabled: true } behaviour)
			{
				continue;
			}
			try
			{
				behaviour.PassGround(going);
			}
			catch (Exception exception)
			{
				Log.Exception(exception, behaviour);
			}
		}
	}
}
