using System.Collections.Generic;
using System.Numerics;
using System.Text.Json;

namespace Voxear;

/// <summary>
/// A town a world of blocks' generator lays out, read with <see cref="Towns.Near(Vector3, float)"/>: its structure,
/// its middle and its buildings, worked out from the generator alone, so a town is known before any of its ground is
/// made, however far off.
/// </summary>
public sealed class Town
{
	internal Town(JsonElement json)
	{
		World = new Obj(json.GetProperty("world").GetUInt64());
		Structure = Structure.FromKey(json.GetProperty("structure").GetString());
		var site = json.GetProperty("site");
		Seed = site.GetProperty("seed").GetUInt32();
		Middle = DungeonJson.Vector(json.GetProperty("middle"));
		var buildings = new List<TownBuilding>();
		foreach (var building in json.GetProperty("buildings").EnumerateArray())
		{
			var door = building.GetProperty("door");
			buildings.Add(new TownBuilding(
				Structure.FromKey(building.GetProperty("structure").GetString()),
				building.GetProperty("middle").GetBoolean(),
				DungeonJson.Vector(building.GetProperty("at")),
				DungeonJson.Vector(building.GetProperty("size")),
				Quaternion.CreateFromAxisAngle(Vector3.UnitY, building.GetProperty("yaw").GetSingle()),
				DungeonJson.Vector(building.GetProperty("front")),
				door.ValueKind == JsonValueKind.Array ? DungeonJson.Vector(door) : null));
		}
		Buildings = buildings;
	}

	/// <summary>The world of blocks it is in.</summary>
	public Obj World { get; }

	/// <summary>The structure that lays it out, as a world's generator lists it.</summary>
	public Structure Structure { get; }

	/// <summary>Its site's own seed: the same town has the same seed every time it is read, on every machine.</summary>
	public uint Seed { get; }

	/// <summary>The middle of its middle building's floor.</summary>
	public Vector3 Middle { get; }

	/// <summary>Its buildings, its middle first, then its houses nearest the middle first.</summary>
	public IReadOnlyList<TownBuilding> Buildings { get; }

	/// <summary>Its houses: every building but its middle.</summary>
	public IEnumerable<TownBuilding> Houses
	{
		get
		{
			foreach (var building in Buildings)
			{
				if (!building.IsMiddle)
				{
					yield return building;
				}
			}
		}
	}

	public override string ToString()
	{
		return $"town of {Buildings.Count} buildings at {Middle}";
	}
}
