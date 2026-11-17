using System.Collections.Generic;
using System.Globalization;
using System.Numerics;
using System.Text.Json;
using Voxear.Interop;

namespace Voxear.Saving;

/// <summary>
/// What a save of the scene keeps of dimensions beside the level's own state: each dungeon's stage by its site, and
/// the Objs left in other dimensions than the level's, each with the dimension it is in. A dimension is named by
/// its asset's GUID and its key, or for a dungeon's, by the dungeon's site: the world's Obj key, which structure,
/// where and its seed, so it is made again the same.
/// </summary>
internal static class DimensionSaves
{
	/// <summary>Every dungeon's stage and cleared flag where either is set, each beside its site.</summary>
	public static IEnumerable<SaveNode> Stages()
	{
		var json = NativeDimensions.Text(NativeDimensions.StagesText, 0);
		if (json == null)
		{
			yield break;
		}
		using var document = JsonDocument.Parse(json);
		foreach (var entry in document.RootElement.EnumerateArray())
		{
			var site = entry.GetProperty("site");
			var world = NativeSaves.Key(NativeSaves.KeyObj, site.GetProperty("world").GetUInt64(), string.Empty);
			if (world == null)
			{
				continue;
			}
			var node = SaveNode.NewTable();
			node["world"] = world;
			node["structure"] = site.GetProperty("structure").GetInt32();
			node["at"] = Pair(site.GetProperty("at"));
			node["stage"] = entry.GetProperty("stage").GetInt32();
			node["cleared"] = entry.GetProperty("cleared").GetBoolean();
			yield return node;
		}
	}

	/// <summary>Puts every dungeon's stage back as <see cref="Stages"/> kept them, and no others.</summary>
	public static void ApplyStages(SaveNode stages)
	{
		var entries = new List<string>();
		foreach (var node in stages.Items)
		{
			var world = NativeSaves.FindObj(node["world"].AsString() ?? string.Empty);
			if (!world.IsValid)
			{
				continue;
			}
			var at = node["at"].Items;
			var (x, z) = (Number(at[0].AsDouble()), Number(at[1].AsDouble()));
			var cleared = node["cleared"].AsBool() ? "true" : "false";
			entries.Add($$"""{"site":{"world":{{world.Id}},"structure":{{node["structure"].AsInt()}},"at":[{{x}},{{z}}]},"stage":{{node["stage"].AsInt()}},"cleared":{{cleared}}}""");
		}
		NativeDimensions.Tell(NativeDimensions.SetStagesText, 0, $"[{string.Join(',', entries)}]");
	}

	/// <summary>The Objs at the top of the hierarchy in other dimensions than the level's, each with its dimension and place.</summary>
	public static IEnumerable<SaveNode> Elsewhere()
	{
		var json = NativeDimensions.Text(NativeDimensions.ElsewhereText, 0);
		if (json == null)
		{
			yield break;
		}
		using var document = JsonDocument.Parse(json);
		foreach (var entry in document.RootElement.EnumerateArray())
		{
			var key = SpawnedKeys.ForSave(NativeSaves.Key(NativeSaves.KeyObj, entry.GetProperty("obj").GetUInt64(), string.Empty));
			var dimension = Name(new DimensionInstance(entry.GetProperty("dimension").GetUInt32()));
			if (key == null || dimension == null)
			{
				continue;
			}
			var at = entry.GetProperty("at");
			var node = SaveNode.NewTable();
			node["obj"] = key;
			node["dimension"] = dimension;
			node["at"] = SaveNode.NewList(at[0].GetDouble(), at[1].GetDouble(), at[2].GetDouble());
			yield return node;
		}
	}

	/// <summary>Moves each Obj <see cref="Elsewhere"/> kept back into its dimension, made first, where it was.</summary>
	public static void MoveBack(SaveNode elsewhere)
	{
		foreach (var node in elsewhere.Items)
		{
			var key = SpawnedKeys.ForLookup(node["obj"].AsString() ?? string.Empty);
			var obj = key == null ? Obj.None : NativeSaves.FindObj(key);
			var making = Make(node["dimension"]);
			if (!obj.IsValid || making == null)
			{
				Log.Warning("A save of the scene has an Obj in another dimension that is not here now, so it is left where it is.");
				continue;
			}
			var at = node["at"].Items;
			var position = new Vector3(at[0].AsFloat(), at[1].AsFloat(), at[2].AsFloat());
			_ = new DimensionMove(obj, making, position);
		}
	}

	/// <summary>A dimension as a save names it; null for the level's, or one made by code that is no dungeon's.</summary>
	private static SaveNode? Name(DimensionInstance dimension)
	{
		if (dimension.IsLevel)
		{
			return null;
		}
		var node = SaveNode.NewTable();
		if (dimension.DungeonSite is { } site)
		{
			var world = NativeSaves.Key(NativeSaves.KeyObj, site.GetProperty("world").GetUInt64(), string.Empty);
			if (world == null)
			{
				return null;
			}
			node["world"] = world;
			node["structure"] = site.GetProperty("structure").GetInt32();
			node["at"] = Pair(site.GetProperty("at"));
			node["seed"] = (long)site.GetProperty("seed").GetUInt32();
			return node;
		}
		if (dimension.Asset.Guid is not { } guid)
		{
			return null;
		}
		node["asset"] = guid;
		node["key"] = dimension.Key.ToString(CultureInfo.InvariantCulture);
		return node;
	}

	/// <summary>Makes the dimension a save names, or gives it made already; null where it names none here.</summary>
	private static DimensionMake? Make(SaveNode name)
	{
		if (name.Has("asset"))
		{
			var key = ulong.TryParse(name["key"].AsString(), NumberStyles.None, CultureInfo.InvariantCulture, out var parsed) ? parsed : 0;
			return Dimensions.Make(Dimension.Load(name["asset"].AsString() ?? string.Empty), key, keepLoaded: false);
		}
		var world = NativeSaves.FindObj(name["world"].AsString() ?? string.Empty);
		if (!world.IsValid)
		{
			return null;
		}
		var at = name["at"].Items;
		var (x, z) = (Number(at[0].AsDouble()), Number(at[1].AsDouble()));
		var site = $$"""{"world":{{world.Id}},"structure":{{name["structure"].AsInt()}},"at":[{{x}},{{z}}],"seed":{{name["seed"].AsLong()}}}""";
		return Dimensions.MakeDungeon(site, keepLoaded: false);
	}

	private static SaveNode Pair(JsonElement at)
	{
		return SaveNode.NewList(at[0].GetDouble(), at[1].GetDouble());
	}

	/// <summary>A number as JSON writes it, whatever the culture.</summary>
	private static string Number(double value)
	{
		return value.ToString("R", CultureInfo.InvariantCulture);
	}
}
