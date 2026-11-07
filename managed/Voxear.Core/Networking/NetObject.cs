using System;
using System.Numerics;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// An Obj's Net Object: it makes the Obj networked, so the server shares it with its players and it moves on their
/// machines as it moves on the server's. Placed in a scene, or on a prefab's root and made on the server with
/// Instantiate, it reaches every player. Give one to a player as theirs with Owner:
/// <code>
/// Net.PlayerJoined += player =>
/// {
/// 	var soldier = soldierPrefab.Instantiate(spawnPoint.WorldPosition, spawnPoint.WorldRotation);
/// 	var net = soldier.NetObject;
/// 	net.Owner = player;
/// };
/// </code>
/// </summary>
public readonly struct NetObject : IEquatable<NetObject>, IObjComponent
{
	public static readonly NetObject None = default;

	internal NetObject(Obj obj)
	{
		Obj = obj;
	}

	public Obj Obj { get; }

	/// <summary>False when its Obj has no Net Object, or is gone.</summary>
	public bool IsValid => Obj.IsValid && NativeNetObject.Number(Obj, NativeNetObject.Has, 0f) != 0f;

	/// <summary>How much it matters next to others when not everything fits in a snapshot: 1 as usual.</summary>
	public float Priority
	{
		get { return NativeNetObject.Number(Obj, NativeNetObject.Priority, 1f); }
		set { NativeNetObject.Set(Obj, NativeNetObject.Priority, new Vector4(value, 0f, 0f, 0f)); }
	}

	/// <summary>How far, in metres, a player's own Objs may be from it for them to be sent it; 0 for everyone.</summary>
	public float Range
	{
		get { return NativeNetObject.Number(Obj, NativeNetObject.Range, 0f); }
		set { NativeNetObject.Set(Obj, NativeNetObject.Range, new Vector4(value, 0f, 0f, 0f)); }
	}

	/// <summary>Its id in the session, the same on every machine; 0 until the server has shared it.</summary>
	public int Id => (int)NativeNetObject.Number(Obj, NativeNetObject.Id, 0f);

	/// <summary>
	/// The player it belongs to, if any, set on the server: theirs is sent to them wherever it is, and where it is
	/// counts as where they are.
	/// </summary>
	public NetPlayer? Owner
	{
		get
		{
			var owner = (int)NativeNetObject.Number(Obj, NativeNetObject.Owner, -1f);
			return owner < 0 ? null : new NetPlayer(owner);
		}
		set { NativeNetObject.Set(Obj, NativeNetObject.Owner, new Vector4(value?.Id ?? -1, 0f, 0f, 0f)); }
	}

	/// <summary>Whether it is this machine's player's own.</summary>
	public bool IsMine
	{
		get
		{
			var owner = Owner;
			return owner.HasValue && owner.Value.IsLocal;
		}
	}

	/// <summary>Takes the Net Object off its Obj; players' copies stay as they are.</summary>
	public void Remove()
	{
		NativeNetObject.Set(Obj, NativeNetObject.Remove, Vector4.Zero);
	}

	public static bool operator ==(NetObject left, NetObject right)
	{
		return left.Obj == right.Obj;
	}

	public static bool operator !=(NetObject left, NetObject right)
	{
		return left.Obj != right.Obj;
	}

	public bool Equals(NetObject other)
	{
		return Obj == other.Obj;
	}

	public override bool Equals(object? other)
	{
		return other is NetObject net && Equals(net);
	}

	public override int GetHashCode()
	{
		return Obj.GetHashCode();
	}

	public override string ToString()
	{
		return $"{Obj}'s Net Object";
	}
}
