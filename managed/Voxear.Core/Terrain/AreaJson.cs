using System.Collections.Generic;
using System.Text.Json.Nodes;
using Voxear.Scripting;

namespace Voxear;

/// <summary>Area tags and their data as a World Area's settings write them.</summary>
internal static class AreaJson
{
	public static IReadOnlyList<AreaEntry> Entries(JsonObject settings)
	{
		var entries = new List<AreaEntry>();
		if (settings["tags"] is not JsonArray read)
		{
			return entries;
		}
		foreach (var node in read)
		{
			var tag = Definitions.Load(typeof(AreaTag), Text(node?["tag"])) as AreaTag;
			var data = Definitions.Load(typeof(Definition), Text(node?["data"]));
			entries.Add(new AreaEntry(tag, data, Biome.FromKey(Text(node?["biome"]))));
		}
		return entries;
	}

	public static JsonArray Of(IEnumerable<AreaEntry> entries)
	{
		var written = new JsonArray();
		foreach (var entry in entries)
		{
			var node = new JsonObject { ["tag"] = Key(entry.Tag) };
			if (entry.Data != null)
			{
				node["data"] = Key(entry.Data);
			}
			if (entry.Biome.IsValid)
			{
				node["biome"] = entry.Biome.Key;
			}
			written.Add(node);
		}
		return written;
	}

	/// <summary>A data asset's GUID, as the engine names it; null for none.</summary>
	public static string? Key(Definition? definition)
	{
		return definition == null || string.IsNullOrEmpty(definition.Guid) ? null : definition.Guid;
	}

	private static string? Text(JsonNode? node)
	{
		return node is JsonValue value && value.TryGetValue<string>(out var text) ? text : null;
	}
}
