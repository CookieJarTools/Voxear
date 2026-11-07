using System;
using System.Numerics;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// A player in networked play, numbered by the server. On the server, their input reads as they pressed it on
/// their own machine, a tick at a time, so the same code moves every player:
/// <code>
/// foreach (var player in Net.Players)
/// {
/// 	var move = player.Vector2("Move");
/// 	if (player.Pressed("Jump"))
/// 	{
/// 		Jump(player);
/// 	}
/// }
/// </code>
/// </summary>
public readonly struct NetPlayer : IEquatable<NetPlayer>
{
	internal NetPlayer(int id)
	{
		Id = id;
	}

	/// <summary>Their number: 0 for a host's own player, and from 1 for those who joined.</summary>
	public int Id { get; }

	/// <summary>Whether this machine plays them.</summary>
	public bool IsLocal
	{
		get { return NativeNet.Call(NativeNet.LocalPlayer) == Id; }
	}

	/// <summary>Whether the action went down this tick.</summary>
	public bool Pressed(string action)
	{
		return (Flags(action) & InputActionData.Pressed) != 0;
	}

	public bool Held(string action)
	{
		return (Flags(action) & InputActionData.Held) != 0;
	}

	public bool Released(string action)
	{
		return (Flags(action) & InputActionData.Released) != 0;
	}

	public float Value(string action)
	{
		return Read(action).X;
	}

	public Vector2 Vector2(string action)
	{
		var state = Read(action);
		return new Vector2(state.X, state.Y);
	}

	/// <summary>
	/// Where they aimed this tick, as their machine set it with Net.Aim: two numbers of the game's choosing, such as
	/// a turn and a tilt in degrees. It comes with their input, so it is where they aimed when they pressed.
	/// </summary>
	public Vector2 Aim
	{
		get
		{
			NativeNet.Call(NativeNet.Aim, Id, 0, out var aim);
			return new Vector2(aim.X, aim.Y);
		}
	}

	public bool Equals(NetPlayer other)
	{
		return Id == other.Id;
	}

	public override bool Equals(object? other)
	{
		return other is NetPlayer player && Equals(player);
	}

	public override int GetHashCode()
	{
		return Id;
	}

	public override string ToString()
	{
		return $"Player {Id}";
	}

	public static bool operator ==(NetPlayer a, NetPlayer b)
	{
		return a.Equals(b);
	}

	public static bool operator !=(NetPlayer a, NetPlayer b)
	{
		return !a.Equals(b);
	}

	private int Flags(string action)
	{
		return (int)Read(action).Z;
	}

	private Vector4 Read(string action)
	{
		var id = NativeInput.Action(action);
		if (id < 0)
		{
			return default;
		}
		NativeNet.Call(NativeNet.Input, Id, id, out var state);
		return state;
	}
}
