using System;
using System.Collections.Generic;
using System.Numerics;
using System.Text.Json.Nodes;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// Prefabs made on the ground of a world of blocks: lined up along a Road, each facing it, no two alike side by side;
/// or scattered over an outline or an Area Fill. Each prefab's Placement says how much ground it takes up and which
/// way its front faces; none is made where something with a Placement or a Ground Fit stands. The ground must be made
/// where they go.
/// <code>
/// var houses = Placing.LineUp(MainStreet, [new PlacingChoice(House, 3), new PlacingChoice(Shop)], new LineUpSettings(Seed: 7));
/// var trees = Placing.Scatter(Park, [new PlacingChoice(Oak)], density: 2f);
/// </code>
/// </summary>
public static class Placing
{
	/// <summary>Lines prefabs up along a Road on the Obj <paramref name="road"/>, and returns what it made.</summary>
	public static Obj[] LineUp(Obj road, IReadOnlyList<PlacingChoice> prefabs, LineUpSettings settings = default)
	{
		settings = settings == default ? new LineUpSettings() : settings;
		var asked = Asked("line_up", prefabs);
		asked["road"] = road.Id;
		asked["side"] = PostJson.Of(settings.Side);
		asked["from"] = settings.From;
		asked["to"] = settings.To;
		asked["setback"] = settings.Setback;
		asked["jitter"] = settings.Jitter;
		asked["gap"] = settings.Gap;
		asked["seed"] = settings.Seed;
		return Make(asked, prefabs);
	}

	/// <summary>Scatters prefabs over the Area Fill on <paramref name="area"/>, about <paramref name="density"/> each 100 square metres.</summary>
	public static Obj[] Scatter(Obj area, IReadOnlyList<PlacingChoice> prefabs, float density = 1f, ulong seed = 1)
	{
		var asked = Asked("scatter", prefabs);
		asked["area"] = area.Id;
		asked["density"] = density;
		asked["seed"] = seed;
		return Make(asked, prefabs);
	}

	/// <summary>Scatters prefabs over an outline along x and z, about <paramref name="density"/> each 100 square metres.</summary>
	public static Obj[] Scatter(IReadOnlyList<Vector2> outline, IReadOnlyList<PlacingChoice> prefabs, float density = 1f, ulong seed = 1)
	{
		var asked = Asked("scatter", prefabs);
		var corners = new JsonArray();
		foreach (var corner in outline)
		{
			corners.Add(PostJson.Of(corner));
		}
		asked["outline"] = corners;
		asked["density"] = density;
		asked["seed"] = seed;
		return Make(asked, prefabs);
	}

	private static JsonObject Asked(string ask, IReadOnlyList<PlacingChoice> prefabs)
	{
		var list = new JsonArray();
		foreach (var choice in prefabs)
		{
			list.Add(new JsonObject { ["prefab"] = ((IAsset<Prefab>)choice.Prefab).AssetGuid, ["weight"] = choice.Weight });
		}
		return new JsonObject { ["ask"] = ask, ["prefabs"] = list };
	}

	private static Obj[] Make(JsonObject asked, IReadOnlyList<PlacingChoice> prefabs)
	{
		var json = NativePost.Call(NativePost.WorldAsk, asked.ToJsonString());
		if (string.IsNullOrEmpty(json) || JsonNode.Parse(json) is not JsonArray spots)
		{
			return [];
		}
		var made = new List<Obj>();
		foreach (var spot in spots)
		{
			var which = spot!["which"]!.GetValue<int>();
			var position = spot["position"]!.AsArray();
			var (x, z) = (position[0]!.GetValue<float>(), position[1]!.GetValue<float>());
			var yaw = spot["yaw"]!.GetValue<float>();
			// On the ground as it is there, found from high over it.
			var y = Blocks.Raycast(new Vector3(x, 2000f, z), -Vector3.UnitY, out var hit, 4000f) ? hit.Point.Y : 0f;
			made.Add(prefabs[which].Prefab.Instantiate(new Vector3(x, y, z), Quaternion.CreateFromAxisAngle(Vector3.UnitY, yaw)));
		}
		return [.. made];
	}
}
