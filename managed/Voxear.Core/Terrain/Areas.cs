using System.Collections.Generic;
using System.Numerics;
using System.Text.Json.Nodes;
using Voxear.Interop;
using Voxear.Scripting;

namespace Voxear;

/// <summary>
/// The World Areas of the dimension the game is seen from, asked about by place.
/// <code>
/// if (Areas.Value&lt;TownLaws&gt;(player.Position, Town) is { } laws &amp;&amp; laws.NoWeapons)
/// {
///     player.Holster();
/// }
/// </code>
/// </summary>
public static class Areas
{
	/// <summary>The Objs of the World Areas holding a place, highest priority first.</summary>
	public static IReadOnlyList<Obj> At(Vector3 at)
	{
		var found = new List<Obj>();
		var asked = new JsonObject { ["ask"] = "areas_at", ["at"] = PostJson.Of(at) };
		var json = NativePost.Call(NativePost.WorldAsk, asked.ToJsonString());
		if (!string.IsNullOrEmpty(json) && JsonNode.Parse(json) is JsonArray objs)
		{
			foreach (var obj in objs)
			{
				found.Add(new Obj(obj!.GetValue<ulong>()));
			}
		}
		return found;
	}

	/// <summary>
	/// The data asset the tag carries at a place: of the first World Area there with the tag, or where none has it, of
	/// the world's Area Defaults for the biome there, then for anywhere. Null where nothing carries the tag; null, with
	/// an error logged, where its data is not a T.
	/// </summary>
	public static T? Value<T>(Vector3 at, AreaTag tag) where T : Definition
	{
		var asked = new JsonObject { ["ask"] = "area_value", ["at"] = PostJson.Of(at), ["tag"] = AreaJson.Key(tag) };
		var json = NativePost.Call(NativePost.WorldAsk, asked.ToJsonString());
		if (string.IsNullOrEmpty(json) || JsonNode.Parse(json) is not JsonObject found)
		{
			return null;
		}
		var data = found["data"] is JsonValue value && value.TryGetValue<string>(out var guid) ? guid : null;
		return Definitions.Load(typeof(T), data) as T;
	}

	/// <summary>Whether any World Area at a place holds the tag, or the world's defaults have it there.</summary>
	public static bool Has(Vector3 at, AreaTag tag)
	{
		var asked = new JsonObject { ["ask"] = "area_value", ["at"] = PostJson.Of(at), ["tag"] = AreaJson.Key(tag) };
		return !string.IsNullOrEmpty(NativePost.Call(NativePost.WorldAsk, asked.ToJsonString()));
	}
}
