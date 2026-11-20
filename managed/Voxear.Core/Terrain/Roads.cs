using System.Numerics;
using System.Text.Json.Nodes;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// The level's Roads, asked about by place.
/// <code>
/// if (Roads.Nearest(Obj.Position, 30f) is { } near)
/// {
///     Log.Info($"{near.Road} passes {Vector3.Distance(near.Point, Obj.Position):0} m off");
/// }
/// </code>
/// </summary>
public static class Roads
{
	/// <summary>The road nearest a place along the ground, within <paramref name="reach"/> metres; null with none.</summary>
	public static RoadHit? Nearest(Vector3 at, float reach = 50f)
	{
		var asked = new JsonObject { ["ask"] = "nearest_road", ["at"] = PostJson.Of(at), ["reach"] = reach };
		var json = NativePost.Call(NativePost.WorldAsk, asked.ToJsonString());
		if (string.IsNullOrEmpty(json) || JsonNode.Parse(json) is not JsonObject found)
		{
			return null;
		}
		var obj = new Obj(found["obj"]!.GetValue<ulong>());
		return new RoadHit(obj, MixJson.Three(found["point"], at), found["along"]?.GetValue<float>() ?? 0f);
	}
}

/// <summary>Where a Road passes nearest a place: its Obj, the point of its middle, and how far along it that is in metres.</summary>
public readonly record struct RoadHit(Obj Road, Vector3 Point, float Along);
