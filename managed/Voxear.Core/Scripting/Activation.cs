using System.Collections.Generic;
using Voxear.Interop;

namespace Voxear.Scripting;

/// <summary>
/// Takes behaviours created during a script call, by Instantiate or AddBehaviour, through Awake and OnEnable
/// before that call returns, as Unity does. The engine's lifecycle is busy running the very callback that made
/// them, so it cannot do this itself. It is done here with the engine's rules instead: Awake runs on the whole
/// batch before OnEnable runs on any of it, both by execution order and then creation order. The batch then
/// goes back to the engine, which runs its Start at the next flush.
/// </summary>
internal static class Activation
{
	private const uint Enabled = 1;
	private const uint WantsEnabled = 2;
	private const uint Destroyed = 4;

	public static void Run(IEnumerable<uint> handles)
	{
		var batch = BehaviourTable.Ordered(handles);
		foreach (var behaviour in batch)
		{
			if (!behaviour.IsDestroyed)
			{
				BehaviourTable.Run(behaviour, Phase.Awake);
			}
		}

		var states = new uint[batch.Count];
		for (var i = 0; i < batch.Count; i++)
		{
			states[i] = Enable(batch[i]);
		}

		var ordered = new uint[batch.Count];
		for (var i = 0; i < batch.Count; i++)
		{
			ordered[i] = batch[i].Handle;
		}
		Native.Adopt(ordered, states);
	}

	/// <summary>
	/// Runs OnEnable unless Awake disabled or destroyed the behaviour, and reports what happened. OnEnable can
	/// itself disable it again, so whether it wants to be enabled is read afterwards.
	/// </summary>
	private static uint Enable(Behaviour behaviour)
	{
		var ran = behaviour.Enabled && !behaviour.IsDestroyed;
		if (ran)
		{
			BehaviourTable.Run(behaviour, Phase.OnEnable);
		}

		var state = ran ? Enabled : 0;
		state |= behaviour.Enabled ? WantsEnabled : 0;
		state |= behaviour.IsDestroyed ? Destroyed : 0;
		return state;
	}
}
