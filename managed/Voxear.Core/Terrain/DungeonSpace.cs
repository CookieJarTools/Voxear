using System;
using System.Numerics;
using System.Text.Json;

namespace Voxear;

/// <summary>
/// A space of a dungeon: a room, a pillared hall, a gorge, a chasm, the boss's arena. Its position is the middle
/// of its floor, a metre over it.
/// </summary>
public sealed class DungeonSpace
{
	internal DungeonSpace(Dungeon dungeon, JsonElement json)
	{
		Dungeon = dungeon;
		Kind = DungeonJson.Kind<DungeonSpaceKind>(json.GetProperty("kind"));
		Floor = json.GetProperty("floor").GetInt32();
		Position = DungeonJson.Vector(json.GetProperty("at"));
		Size = DungeonJson.Vector(json.GetProperty("size"));
		UpperLevel = json.GetProperty("upper").GetSingle();
		Depth = json.GetProperty("below").GetSingle();
		OnMainPath = json.GetProperty("main").GetBoolean();
		Progress = json.GetProperty("progress").GetSingle();
	}

	/// <summary>The dungeon it is part of.</summary>
	public Dungeon Dungeon { get; }

	public DungeonSpaceKind Kind { get; }

	/// <summary>Which floor it is on, from 0 for the first.</summary>
	public int Floor { get; }

	/// <summary>The middle of its floor, a metre over it.</summary>
	public Vector3 Position { get; }

	/// <summary>How far it reaches across, how high it is open over its floor, and how far along.</summary>
	public Vector3 Size { get; }

	/// <summary>The height of its upper ways in where it has them higher than its floor: a gorge's top, a shaft's top.</summary>
	public float UpperLevel { get; }

	/// <summary>How far it goes down under its floor: a chasm's pit, a flooded hall's water.</summary>
	public float Depth { get; }

	/// <summary>Whether it is on the main path from the landing to the end of its floor.</summary>
	public bool OnMainPath { get; }

	/// <summary>How far along its floor it stands: 0 at the start, 1 at the end.</summary>
	public float Progress { get; }

	/// <summary>Whether <paramref name="position"/> is within it.</summary>
	public bool Contains(Vector3 position)
	{
		var offset = position - Position;
		var inside = MathF.Abs(offset.X) <= Size.X * 0.5f && MathF.Abs(offset.Z) <= Size.Z * 0.5f;
		return inside && offset.Y >= -1f - Depth && offset.Y <= Size.Y;
	}

	/// <summary>A place on its floor, away from its walls, picked by <paramref name="random"/>.</summary>
	public Vector3 SpawnPoint(Random? random = null)
	{
		random ??= Random.Shared;
		var x = (random.NextSingle() - 0.5f) * Size.X * 0.6f;
		var z = (random.NextSingle() - 0.5f) * Size.Z * 0.6f;
		return Position + new Vector3(x, 0f, z);
	}
}
