using System;
using System.Text.Json.Nodes;

namespace Voxear;

/// <summary>Project Settings' values read from JSON, laid out as voxear.toml writes them.</summary>
internal static class SettingsJson
{
	public static float Read(JsonObject from, string key, float otherwise)
	{
		var value = otherwise;
		PostJson.Read(from, key, ref value);
		return value;
	}

	public static int Read(JsonObject from, string key, int otherwise)
	{
		var value = otherwise;
		PostJson.Read(from, key, ref value);
		return value;
	}

	public static bool Read(JsonObject from, string key, bool otherwise)
	{
		var value = otherwise;
		PostJson.Read(from, key, ref value);
		return value;
	}

	public static T ReadChoice<T>(JsonObject from, string key, T otherwise) where T : struct, Enum
	{
		var value = otherwise;
		PostJson.ReadChoice(from, key, ref value);
		return value;
	}
}
