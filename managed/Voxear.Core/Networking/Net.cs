using System;
using System.Collections.Generic;
using System.Runtime.Loader;
using Voxear.Interop;
using Voxear.Scripting;

namespace Voxear;

/// <summary>
/// Networked play. Host a game, or serve it without playing, and others join by your address:
/// <code>
/// Net.StartHost();
/// Net.PlayerJoined += player => Log.Info($"{player} joined");
///
/// // On another machine:
/// Net.Connect("192.168.1.20");
/// </code>
/// The game then runs in ticks, its FixedUpdate, at the project's tick rate. On the server each player's input
/// reads through their NetPlayer as they pressed it, and the server decides what happens.
/// </summary>
public static class Net
{
	/// <summary>This client joined its server.</summary>
	public static event Action? Connected;

	/// <summary>This client's session ended, or it could not join.</summary>
	public static event Action? Disconnected;

	/// <summary>Someone joined the game, on the server and every client.</summary>
	public static event Action<NetPlayer>? PlayerJoined;

	public static event Action<NetPlayer>? PlayerLeft;

	public static NetRole Role
	{
		get { return (NetRole)NativeNet.Call(NativeNet.Role); }
	}

	/// <summary>Whether this machine runs the game: a server or a host.</summary>
	public static bool IsServer
	{
		get { return Role is NetRole.Server or NetRole.Host; }
	}

	public static bool IsClient
	{
		get { return Role == NetRole.Client; }
	}

	public static bool IsHost
	{
		get { return Role == NetRole.Host; }
	}

	/// <summary>Whether this client has joined its server; a server always has.</summary>
	public static bool IsConnected
	{
		get { return NativeNet.Call(NativeNet.Connected) == 1; }
	}

	/// <summary>The player this machine plays, if it has one: a host's, or a client's once joined.</summary>
	public static NetPlayer? LocalPlayer
	{
		get
		{
			var id = NativeNet.Call(NativeNet.LocalPlayer);
			return id < 0 ? null : new NetPlayer(id);
		}
	}

	/// <summary>Everyone playing, the local player among them.</summary>
	public static IReadOnlyList<NetPlayer> Players
	{
		get
		{
			var count = NativeNet.Call(NativeNet.PlayerCount);
			var players = new List<NetPlayer>(count);
			for (var at = 0; at < count; at++)
			{
				players.Add(new NetPlayer(NativeNet.Call(NativeNet.PlayerAt, at)));
			}
			return players;
		}
	}

	/// <summary>The session's tick, counted on the server and kept a little ahead of it on a client.</summary>
	public static int Tick
	{
		get { return NativeNet.Call(NativeNet.Tick); }
	}

	/// <summary>How long a message takes to the server and back, in seconds, on a client.</summary>
	public static float RoundTrip
	{
		get
		{
			NativeNet.Call(NativeNet.RoundTrip, 0, 0, out var values);
			return values.X;
		}
	}

	/// <summary>Hosts the game on a port, the project's when 0: it runs here and is played here too.</summary>
	public static bool StartHost(int port = 0)
	{
		return NativeNet.Call(NativeNet.StartHost, port) == 1;
	}

	/// <summary>Serves the game on a port without a player of its own, as a dedicated server does.</summary>
	public static bool StartServer(int port = 0)
	{
		return NativeNet.Call(NativeNet.StartServer, port) == 1;
	}

	/// <summary>
	/// Joins the server at an address, such as "192.168.1.20" or "game.example.com:7777", the project's port when
	/// it names none. Connected or Disconnected says how it went.
	/// </summary>
	public static bool Connect(string address)
	{
		return NativeNet.Call(NativeNet.Connect, address) == 1;
	}

	/// <summary>Ends networked play, telling the other end.</summary>
	public static void Disconnect()
	{
		NativeNet.Call(NativeNet.Stop);
	}

	/// <summary>
	/// Lets go of listeners the scripts added before they were reloaded, which would otherwise run the old code and
	/// keep it in memory.
	/// </summary>
	internal static void Forget(AssemblyLoadContext old)
	{
		Connected = GameCode.Without(Connected, old);
		Disconnected = GameCode.Without(Disconnected, old);
		PlayerJoined = GameCode.Without(PlayerJoined, old);
		PlayerLeft = GameCode.Without(PlayerLeft, old);
	}

	/// <summary>Raises what happened in the session since last frame, and carries replicated properties.</summary>
	internal static void Dispatch()
	{
		Replication.Update();
		while (true)
		{
			var kind = NativeNet.Call(NativeNet.TakeEvent, 0, 0, out var values);
			if (kind == 0)
			{
				return;
			}
			Raise(kind, new NetPlayer((int)values.X));
		}
	}

	private static void Raise(int kind, NetPlayer player)
	{
		switch (kind)
		{
			case NativeNet.ConnectedEvent:
				Connected?.Invoke();
				break;
			case NativeNet.DisconnectedEvent:
				Disconnected?.Invoke();
				break;
			case NativeNet.JoinedEvent:
				PlayerJoined?.Invoke(player);
				break;
			case NativeNet.LeftEvent:
				PlayerLeft?.Invoke(player);
				break;
		}
	}
}
