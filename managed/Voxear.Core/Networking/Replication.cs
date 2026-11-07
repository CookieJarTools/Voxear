using System;
using System.Collections.Generic;
using Voxear.Interop;
using Voxear.Scripting;

namespace Voxear;

/// <summary>
/// Carries replicated properties: on the server, those changed since last frame go to the engine to send; on a
/// player's machine, those that came are set on their behaviours. The code Voxear writes for [Replicated] calls
/// Changed; game code has no need to.
/// </summary>
public static unsafe class Replication
{
	private const int MostBytes = 4096;

	private static readonly HashSet<Behaviour> changed = new HashSet<Behaviour>();

	/// <summary>Notes that a behaviour's replicated property changed, on the server, to send this frame.</summary>
	public static void Changed(Behaviour behaviour)
	{
		if (Net.IsServer)
		{
			changed.Add(behaviour);
		}
	}

	/// <summary>A key for a behaviour's type, the same on every machine: a hash of its full name.</summary>
	internal static uint Key(Type type)
	{
		var hash = 2166136261u;
		foreach (var c in type.FullName ?? type.Name)
		{
			hash = (hash ^ c) * 16777619u;
		}
		return hash;
	}

	/// <summary>Hands the engine what changed, and sets what came.</summary>
	internal static void Update()
	{
		if (!Native.IsConnected)
		{
			return;
		}
		Send();
		TakeIn();
	}

	private static void Send()
	{
		foreach (var behaviour in changed)
		{
			if (behaviour is not IReplicated replicated || !behaviour.Obj.IsValid)
			{
				continue;
			}
			var writer = new NetWriter();
			replicated.WriteReplicated(writer);
			var bytes = writer.ToArray();
			var entity = behaviour.Obj.Id;
			var key = Key(behaviour.GetType());
			fixed (byte* data = bytes)
			{
				Native.Api->NetFields(0, &entity, &key, data, bytes.Length);
			}
		}
		changed.Clear();
	}

	/// <summary>Sets the replicated properties that came on their behaviours.</summary>
	internal static void TakeIn()
	{
		var buffer = new byte[MostBytes];
		while (true)
		{
			ulong entity;
			uint key;
			int length;
			fixed (byte* data = buffer)
			{
				length = Native.Api->NetFields(1, &entity, &key, data, buffer.Length);
			}
			if (length < 0)
			{
				return;
			}
			Apply(new Obj(entity), key, buffer.AsSpan(0, length).ToArray());
		}
	}

	private static void Apply(Obj obj, uint key, byte[] bytes)
	{
		if (!obj.IsValid)
		{
			return;
		}
		foreach (var handle in Native.Behaviours(obj.Id))
		{
			if (BehaviourTable.Get(handle) is IReplicated replicated && Key(replicated.GetType()) == key)
			{
				replicated.ReadReplicated(new NetReader(bytes));
				return;
			}
		}
	}
}
