using System;
using Voxear.Interop;

namespace Voxear.Scripting;

/// <summary>
/// Tells behaviours of the marks their characters' actions passed, as OnMotionMark. The engine sends each mark once
/// for the matcher's Obj and once for each Obj above it, and the enabled behaviours on each are told.
/// </summary>
internal static class MotionMessages
{
	public static void Dispatch(ReadOnlySpan<ulong> objs, string text)
	{
		var marks = text.Split('\n');
		for (var index = 0; index < objs.Length && index < marks.Length; index++)
		{
			Tell(objs[index], MotionMark.Load(marks[index]));
		}
	}

	private static void Tell(ulong obj, MotionMark mark)
	{
		if (!mark.IsValid)
		{
			return;
		}

		foreach (var handle in Native.Behaviours(obj))
		{
			if (BehaviourTable.Get(handle) is { IsDestroyed: false, Enabled: true } behaviour)
			{
				Run(behaviour, mark);
			}
		}
	}

	private static void Run(Behaviour behaviour, MotionMark mark)
	{
		try
		{
			behaviour.PassMark(mark);
		}
		catch (Exception exception)
		{
			Log.Exception(exception, behaviour);
		}
	}
}
