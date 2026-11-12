using System.Collections.Generic;
using System.Text.Json;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// What scripts have read of the project's biomes, by GUID, each read once and again once a biome or a block is
/// saved since: the engine's count of saves says when.
/// </summary>
internal static class BiomeInfos
{
	private static readonly Dictionary<string, BiomeInfo?> known = [];
	private static readonly Dictionary<string, string?> keys = [];
	private static ulong version;

	/// <summary>What the biome <paramref name="key"/> names says; null where the project has no such biome.</summary>
	public static BiomeInfo? Of(string key)
	{
		Fresh();
		if (known.TryGetValue(key, out var found))
		{
			return found;
		}
		if (NativeBiomes.Info(key) is { } json)
		{
			using var document = JsonDocument.Parse(json);
			var color = document.RootElement.GetProperty("color");
			var name = document.RootElement.GetProperty("name").GetString() ?? string.Empty;
			found = new BiomeInfo(name, new Color(color[0].GetSingle(), color[1].GetSingle(), color[2].GetSingle()));
		}
		known[key] = found;
		return found;
	}

	/// <summary>The GUID of the biome at a project path, as saves and comparisons keep it; away from the engine, the path as given.</summary>
	public static string? Key(string reference)
	{
		if (!Native.IsConnected)
		{
			return reference;
		}
		if (!keys.TryGetValue(reference, out var key))
		{
			key = NativeSaves.Key(NativeSaves.KeyReference, 0, reference);
			keys[reference] = key;
		}
		return key;
	}

	/// <summary>Gives a biome made in code its values, as a test does away from the engine.</summary>
	public static Biome Make(string key, BiomeInfo info)
	{
		known[key] = info;
		return Biome.FromKey(key);
	}

	public static void Clear()
	{
		known.Clear();
		keys.Clear();
	}

	private static void Fresh()
	{
		var now = NativeBlocks.Version;
		if (now != version)
		{
			version = now;
			known.Clear();
		}
	}
}
