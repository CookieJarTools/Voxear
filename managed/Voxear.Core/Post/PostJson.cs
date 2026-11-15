using System;
using System.Collections.Generic;
using System.Numerics;
using System.Text;
using System.Text.Json.Nodes;

namespace Voxear;

/// <summary>Post-processing settings to and from JSON, laid out as a scene's [post] writes them.</summary>
internal static class PostJson
{
	public static JsonObject Section(JsonObject from, string key)
	{
		return from[key] as JsonObject ?? [];
	}

	public static JsonObject Node(Action<JsonObject> write)
	{
		var to = new JsonObject();
		write(to);
		return to;
	}

	public static JsonNode Of(Color color)
	{
		return new JsonArray(color.R, color.G, color.B);
	}

	public static JsonNode? Of(Color? color)
	{
		return color is { } set ? Of(set) : null;
	}

	public static JsonNode Of(Vector3 value)
	{
		return new JsonArray(value.X, value.Y, value.Z);
	}

	public static JsonNode? Of(Texture texture)
	{
		return texture.IsSet ? JsonValue.Create(texture.Reference) : null;
	}

	public static JsonNode Of<T>(T choice) where T : struct, Enum
	{
		return JsonValue.Create(Key(choice.ToString()));
	}

	public static JsonNode? Of<T>(T? choice) where T : struct, Enum
	{
		return choice is { } set ? Of(set) : null;
	}

	public static void Read(JsonObject from, string key, ref bool value)
	{
		if (from[key] is JsonValue read && read.TryGetValue(out bool on))
		{
			value = on;
		}
	}

	public static void Read(JsonObject from, string key, ref float value)
	{
		if (Number(from[key]) is { } number)
		{
			value = number;
		}
	}

	public static void Read(JsonObject from, string key, ref int value)
	{
		if (Number(from[key]) is { } number)
		{
			value = (int)MathF.Round(number);
		}
	}

	public static void Read(JsonObject from, string key, ref Color value)
	{
		if (Three(from[key]) is { } rgb)
		{
			value = new Color(rgb.X, rgb.Y, rgb.Z);
		}
	}

	public static void Read(JsonObject from, string key, ref Color? value)
	{
		if (!from.ContainsKey(key))
		{
			return;
		}
		value = Three(from[key]) is { } rgb ? new Color(rgb.X, rgb.Y, rgb.Z) : null;
	}

	public static void Read(JsonObject from, string key, ref Vector3 value)
	{
		if (Three(from[key]) is { } read)
		{
			value = read;
		}
	}

	public static void Read(JsonObject from, string key, ref Texture value)
	{
		if (from[key] is JsonValue read && read.TryGetValue(out string? reference))
		{
			value = string.IsNullOrEmpty(reference) ? Texture.None : Texture.Load(reference);
		}
	}

	public static void ReadChoice<T>(JsonObject from, string key, ref T? value) where T : struct, Enum
	{
		if (!from.ContainsKey(key))
		{
			return;
		}
		T read = default;
		var found = from[key] is JsonValue;
		if (found)
		{
			ReadChoice(from, key, ref read);
		}
		value = found ? read : null;
	}

	public static void ReadChoice<T>(JsonObject from, string key, ref T value) where T : struct, Enum
	{
		if (from[key] is not JsonValue read || !read.TryGetValue(out string? text))
		{
			return;
		}
		foreach (var choice in Enum.GetValues<T>())
		{
			if (Key(choice.ToString()) == text)
			{
				value = choice;
				return;
			}
		}
	}

	/// <summary>A name as files write it: GoldenHour as golden_hour.</summary>
	public static string Key(string name)
	{
		var key = new StringBuilder(name.Length + 4);
		foreach (var letter in name)
		{
			if (char.IsUpper(letter) && key.Length > 0)
			{
				key.Append('_');
			}
			key.Append(char.ToLowerInvariant(letter));
		}
		return key.ToString();
	}

	/// <summary>
	/// What changed between two sets of settings, laid into overrides: each setting whose value is not as it was, as it
	/// now is.
	/// </summary>
	public static void Changed(JsonObject before, JsonObject after, JsonObject into)
	{
		foreach (var (key, now) in after)
		{
			var was = before[key];
			if (now is JsonObject inner)
			{
				var section = into[key] as JsonObject;
				if (section is null)
				{
					section = [];
					into[key] = section;
				}
				Changed(was as JsonObject ?? [], inner, section);
				if (section.Count == 0)
				{
					into.Remove(key);
				}
				continue;
			}
			if (!Same(was, now))
			{
				into[key] = now?.DeepClone();
			}
		}
	}

	/// <summary>Every setting overrides names, as the paths of their sections and keys.</summary>
	public static IEnumerable<string> Paths(JsonObject overrides, string under = "")
	{
		foreach (var (key, value) in overrides)
		{
			var path = under.Length == 0 ? key : $"{under}.{key}";
			if (value is JsonObject inner)
			{
				foreach (var deeper in Paths(inner, path))
				{
					yield return deeper;
				}
				continue;
			}
			yield return path;
		}
	}

	private static bool Same(JsonNode? a, JsonNode? b)
	{
		if (Number(a) is { } x && Number(b) is { } y)
		{
			return MathF.Abs(x - y) <= 1e-6f * MathF.Max(1f, MathF.Abs(x));
		}
		if (a is JsonArray left && b is JsonArray right)
		{
			if (left.Count != right.Count)
			{
				return false;
			}
			for (var i = 0; i < left.Count; i++)
			{
				if (!Same(left[i], right[i]))
				{
					return false;
				}
			}
			return true;
		}
		return JsonNode.DeepEquals(a, b);
	}

	private static float? Number(JsonNode? node)
	{
		if (node is not JsonValue value)
		{
			return null;
		}
		if (value.TryGetValue(out double real))
		{
			return (float)real;
		}
		if (value.TryGetValue(out float single))
		{
			return single;
		}
		if (value.TryGetValue(out int whole))
		{
			return whole;
		}
		if (value.TryGetValue(out long wide))
		{
			return wide;
		}
		return null;
	}

	private static Vector3? Three(JsonNode? node)
	{
		if (node is not JsonArray items || items.Count < 3)
		{
			return null;
		}
		if (Number(items[0]) is not { } x || Number(items[1]) is not { } y || Number(items[2]) is not { } z)
		{
			return null;
		}
		return new Vector3(x, y, z);
	}
}
