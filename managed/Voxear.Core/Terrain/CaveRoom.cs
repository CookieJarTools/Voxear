using System;
using System.Collections.Generic;
using System.Numerics;
using System.Text.Json;

namespace Voxear;

/// <summary>
/// A cave room as it lies in the level: where it is, how it was turned from its prefab, and its exits. Read from
/// <see cref="Caves.Rooms"/> or a <see cref="CaveNetwork"/>.
/// </summary>
public sealed class CaveRoom
{
	private readonly string? prefab;
	private readonly Vector3 floor;
	private readonly Vector3 x;
	private readonly Vector3 z;
	private readonly List<(Vector3 Region, int? Index)> joined = [];
	private Prefab loaded;

	internal CaveRoom(CaveNetwork network, JsonElement json)
	{
		Network = network;
		Index = json.GetProperty("index").ValueKind == JsonValueKind.Number ? json.GetProperty("index").GetInt32() : -1;
		Region = Read(json.GetProperty("region"));
		prefab = json.GetProperty("prefab").GetString();
		Position = Read(json.GetProperty("at"));
		(x, Up, z) = (Read(json.GetProperty("x")), Read(json.GetProperty("y")), Read(json.GetProperty("z")));
		Mirrored = json.GetProperty("mirrored").GetBoolean();
		OnMainPath = json.GetProperty("main").GetBoolean();
		(BoundsMin, BoundsMax) = (Read(json.GetProperty("low")), Read(json.GetProperty("high")));
		floor = Read(json.GetProperty("floor"));
		// The way its prefab faces once placed: its x put back where a mirrored room turned it over.
		var across = Mirrored ? -x : x;
		Rotation = Quaternion.CreateFromRotationMatrix(new Matrix4x4(
			across.X, across.Y, across.Z, 0f,
			Up.X, Up.Y, Up.Z, 0f,
			z.X, z.Y, z.Z, 0f,
			0f, 0f, 0f, 1f));
		var exits = new List<CaveExit>();
		foreach (var exit in json.GetProperty("exits").EnumerateArray())
		{
			var to = exit.GetProperty("to");
			var toIndex = exit.GetProperty("toIndex");
			exits.Add(new CaveExit(
				network,
				Read(exit.GetProperty("at")),
				Read(exit.GetProperty("way")),
				to.ValueKind == JsonValueKind.Array ? Read(to) : null,
				toIndex.ValueKind == JsonValueKind.Number ? toIndex.GetInt32() : null));
		}
		Exits = exits;
		foreach (var other in json.GetProperty("joined").EnumerateArray())
		{
			var toIndex = other.GetProperty("toIndex");
			joined.Add((Read(other.GetProperty("to")), toIndex.ValueKind == JsonValueKind.Number ? toIndex.GetInt32() : null));
		}
	}

	public CaveNetwork Network { get; }

	/// <summary>Its place among a level's rooms, the main path's first; -1 in an endless network.</summary>
	public int Index { get; }

	/// <summary>The region of the network's grid it is in: what tells it from every other room.</summary>
	public Vector3 Region { get; }

	/// <summary>Where its prefab's origin lies.</summary>
	public Vector3 Position { get; }

	/// <summary>How its prefab is turned. A mirrored room is also turned over along its own x, which no rotation says.</summary>
	public Quaternion Rotation { get; }

	public bool Mirrored { get; }

	/// <summary>The way up in it: straight up on flat ground, away from a planet's middle on one.</summary>
	public Vector3 Up { get; }

	/// <summary>Whether it is on a level's main path, rather than off to its side.</summary>
	public bool OnMainPath { get; }

	/// <summary>The corners of a box that holds it.</summary>
	public Vector3 BoundsMin { get; }

	public Vector3 BoundsMax { get; }

	public IReadOnlyList<CaveExit> Exits { get; }

	/// <summary>The prefab it was made from, loaded the first time it is asked for; None for the room a network places while it lists none.</summary>
	public Prefab Prefab
	{
		get
		{
			if (!loaded.IsValid && prefab != null)
			{
				loaded = Prefab.Load(prefab);
			}
			return loaded;
		}
	}

	/// <summary>The rooms a tunnel joins it to. More than its exits lead to, where two tunnels leave by one exit.</summary>
	public IEnumerable<CaveRoom> Neighbours
	{
		get
		{
			foreach (var (region, index) in joined)
			{
				if (Network.Room(region, index) is { } room)
				{
					yield return room;
				}
			}
		}
	}

	/// <summary>A place on its floor with room to stand: where to put a player or an objective.</summary>
	public Vector3 SpawnPoint()
	{
		return floor;
	}

	/// <summary>A place in its space, as it lies in the level: x and z along its floor, y up, from its prefab's origin.</summary>
	public Vector3 Point(Vector3 local)
	{
		return Position + x * local.X + Up * local.Y + z * local.Z;
	}

	/// <summary>A place on its floor some way from its spawn point, no further than <paramref name="reach"/> metres: for scattering things about it.</summary>
	public Vector3 RandomPoint(Random random, float reach = 3f)
	{
		var (along, across) = ((random.NextSingle() * 2f - 1f) * reach, (random.NextSingle() * 2f - 1f) * reach);
		return floor + x * along + z * across;
	}

	private static Vector3 Read(JsonElement json)
	{
		return new Vector3(json[0].GetSingle(), json[1].GetSingle(), json[2].GetSingle());
	}
}
