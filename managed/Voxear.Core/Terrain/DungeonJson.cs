using System;
using System.Numerics;
using System.Text.Json;

namespace Voxear;

/// <summary>How a dungeon's numbers and names read from the engine's JSON.</summary>
internal static class DungeonJson
{
	public static Vector3 Vector(JsonElement json)
	{
		return new Vector3(json[0].GetSingle(), json[1].GetSingle(), json[2].GetSingle());
	}

	public static T Kind<T>(JsonElement json) where T : struct, Enum
	{
		return Enum.TryParse<T>(json.GetString(), out var kind) ? kind : default;
	}
}
