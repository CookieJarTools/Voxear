using System.Collections.Generic;

namespace Voxear;

/// <summary>
/// The level's caves: the rooms a Cave Network node made in the ground of its world of blocks, for putting a
/// player, enemies and objectives in them.
/// <code>
/// Player.Position = Caves.Start!.SpawnPoint();
/// Obj.Instantiate(relic, Caves.End!.SpawnPoint(), Quaternion.Identity);
/// </code>
/// </summary>
public static class Caves
{
	private static CaveNetwork? network;

	/// <summary>The caves of the level's first world of blocks with any; null where there are none.</summary>
	public static CaveNetwork? Network
	{
		get
		{
			// Asked of the engine each time, since a level may have changed: the rooms read of a world are kept
			// while it is still the level's.
			var now = CaveNetwork.Of(0);
			if (now == null || network == null || now.World != network.World || now.Layout != network.Layout)
			{
				network = now;
			}
			return network;
		}
	}

	/// <summary>
	/// The rooms of the level's cave: the main path's first, in order, then its side rooms. Empty where the level
	/// has no cave level; a network without end is asked with <see cref="CaveNetwork.RoomsNear"/>.
	/// </summary>
	public static IReadOnlyList<CaveRoom> Rooms => Network?.Rooms ?? [];

	/// <summary>The first room of the level's main path; null with no cave level.</summary>
	public static CaveRoom? Start => Rooms.Count > 0 ? Rooms[0] : null;

	/// <summary>The last room of the level's main path; null with no cave level.</summary>
	public static CaveRoom? End
	{
		get
		{
			CaveRoom? last = null;
			foreach (var room in Rooms)
			{
				if (room.OnMainPath)
				{
					last = room;
				}
			}
			return last;
		}
	}

	/// <summary>The caves of the world of blocks on an Obj; null where its generator has no Cave Network.</summary>
	public static CaveNetwork? Of(Obj world)
	{
		return CaveNetwork.Of(world.Id);
	}
}
