using System;
using System.Collections.Generic;
using System.Diagnostics;
using Voxear.Interop;
using Voxear.Scripting;

namespace Voxear;

/// <summary>
/// Sends RPCs and runs those that came. The code Voxear writes for [ServerRpc] and [ClientRpc] methods calls this;
/// game code has no need to.
/// </summary>
public static unsafe class Rpc
{
	private const int MostBytes = 8192;
	private const int ToServer = 0;
	private const int ToPlayers = 1;
	private const int Unreliable = 2;
	private const int Take = 4;

	private static readonly Dictionary<(int Player, ulong Obj, uint Key), (double Left, double At)> allowance = [];

	/// <summary>The player whose RPC is running, or null for one from the server.</summary>
	internal static NetPlayer? Caller { get; private set; }

	/// <summary>Asks the server to run a behaviour's method, from a player's machine.</summary>
	public static void Server(Behaviour behaviour, uint key, NetWriter writer, bool reliable)
	{
		Send(behaviour, key, writer, ToServer | (reliable ? 0 : Unreliable), -1);
	}

	/// <summary>Runs a behaviour's method on every player's machine that has its Obj, from the server.</summary>
	public static void Players(Behaviour behaviour, uint key, NetWriter writer, bool reliable)
	{
		Send(behaviour, key, writer, ToPlayers | (reliable ? 0 : Unreliable), -1);
	}

	/// <summary>Runs a behaviour's method on one player's machine, from the server.</summary>
	public static void Player(Behaviour behaviour, NetPlayer player, uint key, NetWriter writer, bool reliable)
	{
		Send(behaviour, key, writer, ToPlayers | (reliable ? 0 : Unreliable), player.Id);
	}

	/// <summary>
	/// Whether a [ServerRpc] that came may run: on the server, made by a player, its Obj's owner unless any player
	/// may, no more often than it allows.
	/// </summary>
	public static bool MayRunOnServer(Behaviour behaviour, uint key, bool anyPlayer, int perSecond)
	{
		if (Caller is not NetPlayer caller || !Net.IsServer)
		{
			return false;
		}
		if (!anyPlayer && behaviour.Obj.NetObject.Owner?.Id != caller.Id)
		{
			return false;
		}
		return perSecond <= 0 || Allowed((caller.Id, behaviour.Obj.Id, key), perSecond);
	}

	/// <summary>Whether a [ClientRpc] that came may run: it came from the server.</summary>
	public static bool MayRunOnPlayer()
	{
		return Caller == null;
	}

	/// <summary>Runs the RPCs that came since last frame.</summary>
	internal static void Receive()
	{
		var buffer = new byte[MostBytes];
		while (true)
		{
			ulong entity;
			uint key;
			int player;
			int length;
			fixed (byte* data = buffer)
			{
				length = Native.Api->NetRpc(Take, &entity, &key, &player, data, buffer.Length);
			}
			if (length < 0)
			{
				Caller = null;
				return;
			}
			Caller = player < 0 ? null : new NetPlayer(player);
			Run(new Obj(entity), key, buffer.AsSpan(0, length).ToArray());
		}
	}

	private static void Send(Behaviour behaviour, uint key, NetWriter writer, int what, int player)
	{
		if (!Native.IsConnected || !behaviour.Obj.IsValid)
		{
			return;
		}
		var bytes = writer.ToArray();
		var entity = behaviour.Obj.Id;
		fixed (byte* data = bytes)
		{
			Native.Api->NetRpc(what, &entity, &key, &player, data, bytes.Length);
		}
	}

	private static void Run(Obj obj, uint key, byte[] bytes)
	{
		if (!obj.IsValid)
		{
			return;
		}
		foreach (var handle in Native.Behaviours(obj.Id))
		{
			if (BehaviourTable.Get(handle) is IRpcs rpcs && Ran(rpcs, key, bytes))
			{
				return;
			}
		}
	}

	private static bool Ran(IRpcs rpcs, uint key, byte[] bytes)
	{
		try
		{
			return rpcs.ReceiveRpc(key, new NetReader(bytes));
		}
		catch (Exception exception)
		{
			Log.Exception(exception);
			return true;
		}
	}

	private static bool Allowed((int, ulong, uint) at, int perSecond)
	{
		var now = Stopwatch.GetTimestamp() / (double)Stopwatch.Frequency;
		if (allowance.Count > 4096)
		{
			allowance.Clear();
		}
		var (left, then) = allowance.TryGetValue(at, out var held) ? held : (perSecond, now);
		left = Math.Min(perSecond, left + (now - then) * perSecond);
		var allowed = left >= 1.0;
		allowance[at] = (allowed ? left - 1.0 : left, now);
		return allowed;
	}
}
