using System.Collections.Generic;
using System.Numerics;
using System.Text.Json.Nodes;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// The Routes of the dimension the game is seen from, and their Roads, asked about by place.
/// <code>
/// if (Routes.Find(Obj.Position, market.Position, RouteKind.Road) is { } way)
/// {
///     Log.Info($"{way.Length:0} m, by {way.Stops.Count} stops");
/// }
/// </code>
/// </summary>
public static class Routes
{
	/// <summary>The way from one place to another along routes of a kind; null where none joins them.</summary>
	public static RouteWay? Find(Vector3 from, Vector3 to, RouteKind kind = RouteKind.Road)
	{
		return FindOf(from, to, PostJson.Of(kind));
	}

	/// <summary>The way from one place to another along routes of a kind of the game's own.</summary>
	public static RouteWay? Find(Vector3 from, Vector3 to, CustomRouteKind kind)
	{
		return FindOf(from, to, AreaJson.Key(kind));
	}

	/// <summary>The nearest place on a route of a kind; null with none.</summary>
	public static RouteHit? Nearest(Vector3 at, RouteKind kind = RouteKind.Road)
	{
		return NearestOf(at, PostJson.Of(kind));
	}

	/// <summary>The nearest place on a route of a kind of the game's own; null with none.</summary>
	public static RouteHit? Nearest(Vector3 at, CustomRouteKind kind)
	{
		return NearestOf(at, AreaJson.Key(kind));
	}

	/// <summary>
	/// A way over the sea between two places, at the sea's height, over water deeper than <paramref name="draught"/> and
	/// further than <paramref name="shore"/> metres from land, searched over the world's generated ground however far
	/// off; null where the sea does not join them.
	/// </summary>
	public static IReadOnlyList<Vector3>? LaySea(Vector3 from, Vector3 to, float draught = 3f, float shore = 24f)
	{
		var asked = new JsonObject
		{
			["ask"] = "route_lay_sea",
			["from"] = PostJson.Of(from),
			["to"] = PostJson.Of(to),
			["draught"] = draught,
			["shore"] = shore,
		};
		var json = NativePost.Call(NativePost.WorldAsk, asked.ToJsonString());
		return string.IsNullOrEmpty(json) || JsonNode.Parse(json) is not JsonArray points ? null : PointsOf(points);
	}

	private static RouteWay? FindOf(Vector3 from, Vector3 to, JsonNode? kind)
	{
		var asked = new JsonObject { ["ask"] = "route_find", ["from"] = PostJson.Of(from), ["to"] = PostJson.Of(to), ["kind"] = kind };
		var json = NativePost.Call(NativePost.WorldAsk, asked.ToJsonString());
		if (string.IsNullOrEmpty(json) || JsonNode.Parse(json) is not JsonObject found)
		{
			return null;
		}
		var stops = new List<Obj>();
		if (found["stops"] is JsonArray read)
		{
			foreach (var stop in read)
			{
				stops.Add(new Obj(stop!.GetValue<ulong>()));
			}
		}
		var points = found["points"] is JsonArray list ? PointsOf(list) : [];
		return new RouteWay(points, found["length"]?.GetValue<float>() ?? 0f, stops);
	}

	private static RouteHit? NearestOf(Vector3 at, JsonNode? kind)
	{
		var asked = new JsonObject { ["ask"] = "route_nearest", ["at"] = PostJson.Of(at), ["kind"] = kind };
		var json = NativePost.Call(NativePost.WorldAsk, asked.ToJsonString());
		if (string.IsNullOrEmpty(json) || JsonNode.Parse(json) is not JsonObject found)
		{
			return null;
		}
		var obj = new Obj(found["obj"]!.GetValue<ulong>());
		return new RouteHit(obj, MixJson.Three(found["point"], at), found["along"]?.GetValue<float>() ?? 0f);
	}

	private static List<Vector3> PointsOf(JsonArray points)
	{
		var found = new List<Vector3>();
		foreach (var point in points)
		{
			found.Add(MixJson.Three(point, Vector3.Zero));
		}
		return found;
	}
}

/// <summary>The nearest place on a route: the route's Obj, the place, and how far along the route it lies in metres.</summary>
public readonly record struct RouteHit(Obj Route, Vector3 Point, float Along);
