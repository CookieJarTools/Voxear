using System.Collections.Generic;
using System.Text.Json;
using System.Numerics;

namespace Voxear;

/// <summary>A way between two spaces of a dungeon, or from its entrance: the middle line of its floor.</summary>
public sealed class DungeonWay
{
	private readonly int from;
	private readonly int to;

	internal DungeonWay(Dungeon dungeon, JsonElement json)
	{
		Dungeon = dungeon;
		Kind = DungeonJson.Kind<DungeonWayKind>(json.GetProperty("kind"));
		OnMainPath = json.GetProperty("main").GetBoolean();
		from = json.GetProperty("from").GetInt32();
		to = json.GetProperty("to").GetInt32();
		Width = json.GetProperty("width").GetSingle();
		var points = new List<Vector3>();
		foreach (var point in json.GetProperty("points").EnumerateArray())
		{
			points.Add(DungeonJson.Vector(point));
		}
		Points = points;
	}

	public Dungeon Dungeon { get; }

	public DungeonWayKind Kind { get; }

	public bool OnMainPath { get; }

	public float Width { get; }

	/// <summary>The middle line of its floor, from one end to the other.</summary>
	public IReadOnlyList<Vector3> Points { get; }

	/// <summary>The space it starts in; null for the grand stair, which starts at the entrance.</summary>
	public DungeonSpace? From => from >= 0 && from < Dungeon.Spaces.Count ? Dungeon.Spaces[from] : null;

	/// <summary>The space it ends in; null for a way broken through to a cave.</summary>
	public DungeonSpace? To => to >= 0 && to < Dungeon.Spaces.Count ? Dungeon.Spaces[to] : null;
}
