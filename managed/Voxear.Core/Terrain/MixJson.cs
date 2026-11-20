using System.Collections.Generic;
using System.Numerics;
using System.Text.Json.Nodes;

namespace Voxear;

/// <summary>Mixes, patterns and lists of points as Roads and Area Fills keep them.</summary>
internal static class MixJson
{
	public static IReadOnlyList<MixBlock> Mix(JsonObject settings)
	{
		var mix = new List<MixBlock>();
		if (settings["mix"] is JsonArray read)
		{
			foreach (var node in read)
			{
				var block = node?["block"] is JsonValue guid && guid.TryGetValue<string>(out var key) ? Block.FromKey(key) : Block.None;
				var weight = node?["weight"] is JsonValue w && w.TryGetValue<int>(out var number) ? number : 1;
				mix.Add(new MixBlock(block, weight));
			}
		}
		return mix;
	}

	public static JsonArray Of(IEnumerable<MixBlock> mix)
	{
		var array = new JsonArray();
		foreach (var entry in mix)
		{
			array.Add(new JsonObject { ["block"] = entry.Block.IsValid ? entry.Block.Key : null, ["weight"] = entry.Weight });
		}
		return array;
	}

	public static IReadOnlyList<string> Pattern(JsonObject settings)
	{
		var rows = new List<string>();
		if (settings["pattern"] is JsonArray read)
		{
			foreach (var node in read)
			{
				if (node is JsonValue row && row.TryGetValue<string>(out var text))
				{
					rows.Add(text);
				}
			}
		}
		return rows;
	}

	public static JsonArray Of(IEnumerable<string> rows)
	{
		var array = new JsonArray();
		foreach (var row in rows)
		{
			array.Add(row);
		}
		return array;
	}

	public static Vector3 Three(JsonNode? node, Vector3 otherwise)
	{
		return node is JsonArray { Count: 3 } v ? new Vector3(v[0]!.GetValue<float>(), v[1]!.GetValue<float>(), v[2]!.GetValue<float>()) : otherwise;
	}

	public static Block BlockAt(JsonObject settings, string key)
	{
		return settings[key] is JsonValue value && value.TryGetValue<string>(out var guid) ? Block.FromKey(guid) : Block.None;
	}
}
