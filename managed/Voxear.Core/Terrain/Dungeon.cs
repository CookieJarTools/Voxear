using System.Collections.Generic;
using System.Numerics;
using System.Text.Json;

namespace Voxear;

/// <summary>
/// A dungeon as a world of blocks lays it out: its entrance at the surface, its floors, its spaces from the first
/// floor's landing down to the boss's arena, the ways between them and its lights. Read from <see cref="Dungeons"/>.
/// </summary>
public sealed class Dungeon
{
	internal Dungeon(JsonElement json)
	{
		World = json.GetProperty("world").GetUInt64();
		Floors = json.GetProperty("floors").GetInt32();
		var entrance = json.GetProperty("entrance");
		Entrance = new DungeonEntrance(
			DungeonJson.Kind<DungeonEntranceKind>(entrance.GetProperty("kind")),
			DungeonJson.Vector(entrance.GetProperty("at")),
			DungeonJson.Vector(entrance.GetProperty("facing")));
		var spaces = new List<DungeonSpace>();
		foreach (var space in json.GetProperty("spaces").EnumerateArray())
		{
			spaces.Add(new DungeonSpace(this, space));
		}
		Spaces = spaces;
		var ways = new List<DungeonWay>();
		foreach (var way in json.GetProperty("ways").EnumerateArray())
		{
			ways.Add(new DungeonWay(this, way));
		}
		Ways = ways;
		var lights = new List<DungeonLight>();
		foreach (var light in json.GetProperty("lights").EnumerateArray())
		{
			lights.Add(new DungeonLight(DungeonJson.Kind<DungeonLightKind>(light.GetProperty("kind")), DungeonJson.Vector(light.GetProperty("at"))));
		}
		Lights = lights;
	}

	/// <summary>The Obj id of the world of blocks it is in.</summary>
	public ulong World { get; }

	/// <summary>How many floors it goes down.</summary>
	public int Floors { get; }

	/// <summary>Its entrance at the surface: the only part of it the land does not cover.</summary>
	public DungeonEntrance Entrance { get; }

	/// <summary>Its spaces: the first floor's landing first, then each floor's in the order of its graph.</summary>
	public IReadOnlyList<DungeonSpace> Spaces { get; }

	/// <summary>The ways between its spaces: the grand stair from the entrance, passages and shortcuts.</summary>
	public IReadOnlyList<DungeonWay> Ways { get; }

	/// <summary>Its braziers, crystals and the glows of its set pieces.</summary>
	public IReadOnlyList<DungeonLight> Lights { get; }

	/// <summary>The landing at the foot of the grand stair.</summary>
	public DungeonSpace? Start => Spaces.Count > 0 ? Spaces[0] : null;

	/// <summary>The boss's arena at the end of the last floor; null where it has none.</summary>
	public DungeonSpace? Boss
	{
		get
		{
			foreach (var space in Spaces)
			{
				if (space.Kind == DungeonSpaceKind.Arena)
				{
					return space;
				}
			}
			return null;
		}
	}

	/// <summary>Its spaces of <paramref name="kind"/>.</summary>
	public IEnumerable<DungeonSpace> SpacesOf(DungeonSpaceKind kind)
	{
		foreach (var space in Spaces)
		{
			if (space.Kind == kind)
			{
				yield return space;
			}
		}
	}

	/// <summary>The space <paramref name="position"/> is inside; null where it is in none.</summary>
	public DungeonSpace? SpaceAt(Vector3 position)
	{
		foreach (var space in Spaces)
		{
			if (space.Contains(position))
			{
				return space;
			}
		}
		return null;
	}
}
