using System.Text.Json.Nodes;

namespace Voxear.Interop;

/// <summary>A setting at a time of the level's own Seasons or Climate, the first in the level.</summary>
internal sealed class LevelSettings(string component, string title)
{
	/// <summary>Whether the level has one.</summary>
	public bool Exists => NativeWhole.Level(component) is not null;

	public float Number(string key, float otherwise)
	{
		var value = otherwise;
		PostJson.Read(Read(), key, ref value);
		return value;
	}

	public Color Colour(string key, Color otherwise)
	{
		var value = otherwise;
		PostJson.Read(Read(), key, ref value);
		return value;
	}

	public JsonObject Read()
	{
		return NativeWhole.Level(component) ?? [];
	}

	public void Change(string key, JsonNode? value)
	{
		if (NativeWhole.Level(component, new JsonObject { [key] = value }) is null && Native.IsConnected)
		{
			Log.Warning($"{title}: the level has no {title} to change");
		}
	}
}
