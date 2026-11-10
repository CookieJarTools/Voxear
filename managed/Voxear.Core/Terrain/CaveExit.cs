using System.Numerics;

namespace Voxear;

/// <summary>One of a cave room's exits: where a tunnel may leave it, and which room that tunnel leads to.</summary>
public sealed class CaveExit
{
	private readonly CaveNetwork network;
	private readonly Vector3? region;
	private readonly int? index;

	internal CaveExit(CaveNetwork network, Vector3 position, Vector3 direction, Vector3? region, int? index)
	{
		(this.network, Position, Direction) = (network, position, direction);
		(this.region, this.index) = (region, index);
	}

	public Vector3 Position { get; }

	/// <summary>The way out of the room, one long.</summary>
	public Vector3 Direction { get; }

	/// <summary>Whether a tunnel leaves by it.</summary>
	public bool Used => region != null;

	/// <summary>The room its tunnel leads to, the first of them where more than one tunnel leaves by it; null for an exit no tunnel uses.</summary>
	public CaveRoom? LeadsTo => region is { } to ? network.Room(to, index) : null;
}
