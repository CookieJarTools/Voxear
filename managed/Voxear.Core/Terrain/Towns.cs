using System.Collections.Generic;
using System.Numerics;
using System.Text.Json;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// The towns the level's worlds of blocks build: their houses, where each one's door is and which way it faces, for
/// a game to people them. Worked out from the generator, so a town far off, never visited, is known as it will be
/// built.
/// <code>
/// foreach (var town in Towns.Near(Player.Position, 2000f))
/// {
///     foreach (var house in town.Houses)
///     {
///         society.AddHome(town.Seed, house.Structure, house.Door!.Value);
///     }
/// }
/// </code>
/// </summary>
public static class Towns
{
	/// <summary>The towns of every world of blocks of the dimension asked of whose sites lie within <paramref name="reach"/> metres.</summary>
	public static IReadOnlyList<Town> Near(Vector3 position, float reach = 500f)
	{
		return Read(0, position, reach);
	}

	/// <summary>The towns of the world of blocks on <paramref name="world"/> whose sites lie within <paramref name="reach"/> metres.</summary>
	public static IReadOnlyList<Town> Near(Obj world, Vector3 position, float reach = 500f)
	{
		return Read(world.Id, position, reach);
	}

	/// <summary>The town whose middle is nearest, within <paramref name="reach"/> metres; null where none is.</summary>
	public static Town? Nearest(Vector3 position, float reach = 1000f)
	{
		Town? best = null;
		var least = float.MaxValue;
		foreach (var town in Near(position, reach))
		{
			var distance = Vector3.Distance(town.Middle, position);
			if (distance < least)
			{
				(best, least) = (town, distance);
			}
		}
		return best;
	}

	private static IReadOnlyList<Town> Read(ulong world, Vector3 position, float reach)
	{
		var json = NativeTowns.Read(world, new Vector4(position, reach));
		if (json == null)
		{
			return [];
		}
		using var document = JsonDocument.Parse(json);
		var found = new List<Town>();
		foreach (var town in document.RootElement.GetProperty("towns").EnumerateArray())
		{
			found.Add(new Town(town));
		}
		return found;
	}
}
