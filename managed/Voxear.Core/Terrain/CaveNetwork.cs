using System.Collections.Generic;
using System.Numerics;
using System.Text.Json;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// The caves a world of blocks' generator made with a Cave Network node: its rooms, where they lie in the level.
/// <see cref="Caves"/> is the short way to the level's own; this is for a level with more than one world of
/// blocks, and for finding rooms in a network without end.
/// </summary>
public sealed class CaveNetwork
{
	private readonly Dictionary<Vector3, CaveRoom> known = [];
	private List<CaveRoom>? rooms;

	internal CaveNetwork(Obj world, CaveLayout layout)
	{
		(World, Layout) = (world, layout);
	}

	/// <summary>The Obj of the world of blocks.</summary>
	public Obj World { get; }

	public CaveLayout Layout { get; }

	/// <summary>A level's rooms: the main path's first, in order, then its side rooms. Empty for a network without end.</summary>
	public IReadOnlyList<CaveRoom> Rooms => rooms ??= Read(NativeCaves.Level, default);

	/// <summary>The rooms whose origin is within <paramref name="radius"/> metres of a place, nearest first, in either layout.</summary>
	public IReadOnlyList<CaveRoom> RoomsNear(Vector3 position, float radius)
	{
		return Read(NativeCaves.Near, new Vector4(position, radius));
	}

	/// <summary>The room a place is inside; null in a tunnel or in rock.</summary>
	public CaveRoom? RoomAt(Vector3 position)
	{
		var found = Read(NativeCaves.Inside, new Vector4(position, 0f));
		return found.Count > 0 ? found[0] : null;
	}

	/// <summary>The room of a region, for an exit's <see cref="CaveExit.LeadsTo"/>.</summary>
	internal CaveRoom? Room(Vector3 region, int? index)
	{
		if (index is { } at && at < Rooms.Count)
		{
			return Rooms[at];
		}
		if (known.TryGetValue(region, out var room))
		{
			return room;
		}
		var found = Read(NativeCaves.Region, new Vector4(region, 0f));
		return found.Count > 0 ? found[0] : null;
	}

	/// <summary>The network of the world of blocks on <paramref name="world"/>, or of the first with one; null where there is none.</summary>
	internal static CaveNetwork? Of(ulong world)
	{
		var json = NativeCaves.Rooms(world, NativeCaves.None, default);
		if (json == null)
		{
			return null;
		}
		using var document = JsonDocument.Parse(json);
		var root = document.RootElement;
		var layout = root.GetProperty("layout").GetString() == "level" ? CaveLayout.Level : CaveLayout.Endless;
		return new CaveNetwork(new Obj(root.GetProperty("obj").GetUInt64()), layout);
	}

	private List<CaveRoom> Read(int what, Vector4 at)
	{
		var read = new List<CaveRoom>();
		var json = NativeCaves.Rooms(World.Id, what, at);
		if (json == null)
		{
			return read;
		}
		using var document = JsonDocument.Parse(json);
		foreach (var room in document.RootElement.GetProperty("rooms").EnumerateArray())
		{
			var made = new CaveRoom(this, room);
			// One room is one object, however often it is read.
			if (known.TryGetValue(made.Region, out var before))
			{
				made = before;
			}
			else
			{
				known[made.Region] = made;
			}
			read.Add(made);
		}
		return read;
	}
}
