using System.Collections.Generic;
using System.Numerics;
using System.Text.Json;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// The dungeons the level's worlds of blocks build: where each one's entrance is, its spaces, the ways between
/// them and its lights, for putting players, enemies and loot in them.
/// <code>
/// var dungeon = Dungeons.Nearest(Player.Position);
/// Obj.Instantiate(boss, dungeon!.Boss!.Position, Quaternion.Identity);
/// foreach (var room in dungeon.SpacesOf(DungeonSpaceKind.Room))
/// {
/// 	SpawnGuards(room.Position, room.Size);
/// }
/// </code>
/// </summary>
public static class Dungeons
{
	/// <summary>
	/// The dungeons of every world of blocks of the dimension the game is seen from within <paramref name="reach"/>
	/// metres. In the level, a dungeon in a dimension of its own is its gate alone; inside its dimension, it is listed
	/// whole, its spaces in that dimension's coordinates.
	/// </summary>
	public static IReadOnlyList<Dungeon> Near(Vector3 position, float reach = 500f)
	{
		return Read(0, position, reach);
	}

	/// <summary>The dungeons of the world of blocks on <paramref name="world"/> within <paramref name="reach"/> metres.</summary>
	public static IReadOnlyList<Dungeon> Near(Obj world, Vector3 position, float reach = 500f)
	{
		return Read(world.Id, position, reach);
	}

	/// <summary>The dungeon whose entrance is nearest, within <paramref name="reach"/> metres; null where none is.</summary>
	public static Dungeon? Nearest(Vector3 position, float reach = 1000f)
	{
		Dungeon? best = null;
		var least = float.MaxValue;
		foreach (var dungeon in Near(position, reach))
		{
			var distance = Vector3.Distance(dungeon.Entrance.Position, position);
			if (distance < least)
			{
				(best, least) = (dungeon, distance);
			}
		}
		return best;
	}

	/// <summary>The space of a dungeon <paramref name="position"/> is inside; null where it is in none.</summary>
	public static DungeonSpace? SpaceAt(Vector3 position)
	{
		foreach (var dungeon in Near(position, 1f))
		{
			if (dungeon.SpaceAt(position) is { } space)
			{
				return space;
			}
		}
		return null;
	}

	private static IReadOnlyList<Dungeon> Read(ulong world, Vector3 position, float reach)
	{
		var json = NativeDungeons.Read(world, new Vector4(position, reach));
		if (json == null)
		{
			return [];
		}
		using var document = JsonDocument.Parse(json);
		var found = new List<Dungeon>();
		foreach (var dungeon in document.RootElement.GetProperty("dungeons").EnumerateArray())
		{
			found.Add(new Dungeon(dungeon));
		}
		return found;
	}
}
