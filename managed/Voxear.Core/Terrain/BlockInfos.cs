using System.Collections.Generic;
using System.Text.Json;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// What scripts have read of the project's blocks, by GUID, each read once and again once a block is saved since:
/// the engine's count of saves says when.
/// </summary>
internal static class BlockInfos
{
	private static readonly Dictionary<string, BlockInfo?> known = [];
	private static readonly Dictionary<string, string?> keys = [];
	private static readonly Dictionary<(ulong Obj, ushort Number), Block> numbered = [];
	private static ulong version;

	/// <summary>What the block <paramref name="key"/> names says; null where the project has no such block.</summary>
	public static BlockInfo? Of(string key)
	{
		Fresh();
		if (known.TryGetValue(key, out var found))
		{
			return found;
		}
		var json = NativeBlocks.Info(key);
		if (json != null)
		{
			using var document = JsonDocument.Parse(json);
			found = new BlockInfo(document.RootElement);
		}
		known[key] = found;
		return found;
	}

	/// <summary>The GUID of the block at a project path, as saves and comparisons keep it; away from the engine, the path as given.</summary>
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

	/// <summary>The block numbered <paramref name="number"/> in the world on <paramref name="obj"/>; None for air, or a kind of the world's own.</summary>
	public static Block Numbered(Obj obj, ushort number)
	{
		if (number == Blocks.Air || !Native.IsConnected)
		{
			return Block.None;
		}
		Fresh();
		if (!numbered.TryGetValue((obj.Id, number), out var block))
		{
			block = Block.FromKey(NativeBlocks.Of(obj.Id, number) ?? string.Empty);
			numbered[(obj.Id, number)] = block;
		}
		return block;
	}

	/// <summary>Gives a block made in code its values, as a test does away from the engine.</summary>
	public static Block Make(string key, BlockInfo info)
	{
		known[key] = info;
		return Block.FromKey(key);
	}

	/// <summary>
	/// Forgets which block a number is in each world, as after a world numbered a block it did not list: a number is
	/// given once and never changes, so only what was not a block before can be one now.
	/// </summary>
	public static void Renumbered()
	{
		numbered.Clear();
	}

	public static void Clear()
	{
		known.Clear();
		keys.Clear();
		numbered.Clear();
	}

	private static void Fresh()
	{
		var now = NativeBlocks.Version;
		if (now != version)
		{
			version = now;
			known.Clear();
			numbered.Clear();
		}
	}
}
