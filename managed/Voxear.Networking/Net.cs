using System;
using System.Collections.Generic;
using System.Numerics;
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
public static unsafe class Net
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

	/// <summary>Who decides what a player's own Objs do, as the project's [network] settings say: the server, or shared.</summary>
	public static NetAuthority Authority
	{
		get { return (NetAuthority)NativeNet.Call(NativeNet.Authority); }
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

	/// <summary>
	/// Whether this machine is running its [Predicted] ticks again, after the server put its player's Obj right: an
	/// effect such as a sound should play only when this is false, as it played the first time.
	/// </summary>
	public static bool IsReplaying
	{
		get { return NativeNet.Call(NativeNet.Replaying) == 1; }
	}

	/// <summary>
	/// Where this machine's player aims, sent with their input each tick: two numbers of the game's choosing, such as
	/// a turn and a tilt in degrees, which the server reads as NetPlayer.Aim. Mouse movement is better sent this way,
	/// as where it ended up, than as an action, since a lost tick of movement would leave the aim off for good.
	/// <code>
	/// yaw += Mouse.Delta.X * sensitivity;
	/// Net.Aim = new Vector2(yaw, pitch);
	/// </code>
	/// </summary>
	public static Vector2 Aim
	{
		get { return LocalPlayer?.Aim ?? Vector2.Zero; }
		set { NativeNet.Call(NativeNet.SetAim, BitConverter.SingleToInt32Bits(value.X), BitConverter.SingleToInt32Bits(value.Y)); }
	}

	/// <summary>The player whose [ServerRpc] is running, on the server; null otherwise.</summary>
	public static NetPlayer? Caller
	{
		get { return Rpc.Caller; }
	}

	/// <summary>The tick the running RPC was made on, by the clock of the machine that made it.</summary>
	public static int CallTick
	{
		get { return NativeNet.Call(NativeNet.RpcTick); }
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

	/// <summary>
	/// The first collider a ray meets as a player's machine showed the world, on the server: other players and shared
	/// Objs where that player saw them, a moment in the past, rather than where they have got to since. Check a
	/// player's shot this way, in a [ServerRpc], so that one aimed true hits:
	/// <code>
	/// [ServerRpc]
	/// private void Fire(Vector3 aim)
	/// {
	/// 	if (Net.RaycastAsSeenBy(Net.Caller!.Value, Obj.WorldPosition, aim, out var hit, 200f))
	/// 	{
	/// 		hit.Obj.GetBehaviour&lt;Health&gt;()?.Hurt(20);
	/// 	}
	/// }
	/// </code>
	/// The world is put back no more than 0.2 seconds, so a player with a slow connection cannot hit someone long
	/// since behind a wall. Their own Objs stay where they are. Elsewhere, and for a host's own player, it casts as
	/// Physics.Raycast does.
	/// </summary>
	public static bool RaycastAsSeenBy(NetPlayer player, Vector3 origin, Vector3 direction, out RaycastHit hit, float maxDistance = float.PositiveInfinity, int layerMask = -1, bool hitTriggers = false)
	{
		var tick = Caller?.Id == player.Id ? CallTick : Tick;
		var found = default(RaycastHitData);
		var met = Native.IsConnected
			&& Native.Api->NetRaycast(player.Id, tick, (float*)&origin, (float*)&direction, maxDistance, (uint)layerMask, hitTriggers ? 1 : 0, &found) == 1;
		if (!met)
		{
			hit = default;
			return false;
		}
		var point = new Vector3(found.PointX, found.PointY, found.PointZ);
		var normal = new Vector3(found.NormalX, found.NormalY, found.NormalZ);
		hit = new RaycastHit(new Obj(found.Obj), point, normal, found.Distance);
		return true;
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

	/// <summary>Raises what happened in the session since last frame, carries replicated properties and runs RPCs.</summary>
	internal static void Dispatch()
	{
		Replication.Update();
		Rpc.Receive();
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
