using System;
using System.Collections.Generic;

namespace Voxear;

/// <summary>
/// A block: a .block file with how it looks and its rules, which the worlds of blocks made of it list. A script
/// holds one in a field, picked in the Inspector, or gets the one a ray met from <see cref="BlockHit.Definition"/>.
/// <code>
/// public Block Stone;
/// ...
/// if (Blocks.Raycast(eye, forward, out var hit, 6f) &amp;&amp; hit.Definition == Stone)
/// {
///     Log.Info($"{hit.Definition.Name} takes {hit.Definition.BreakTime(held):0.0} s to break");
/// }
/// </code>
/// </summary>
public readonly struct Block : IEquatable<Block>, IAsset<Block>
{
	public static readonly Block None = default;

	/// <summary>The block's GUID, or away from the engine its path.</summary>
	private readonly string? key;

	private Block(string key)
	{
		this.key = key;
	}

	/// <summary>Whether it is a block: false for none, for air, and for a kind a world keeps itself.</summary>
	public bool IsValid => !string.IsNullOrEmpty(key);

	internal string Key => key ?? string.Empty;

	string? IAsset<Block>.AssetGuid => IsValid ? key : null;

	private BlockInfo? Info => IsValid ? BlockInfos.Of(key!) : null;

	/// <summary>What it is called: its file's name.</summary>
	public string Name => Info?.Name ?? string.Empty;

	/// <summary>Its colour where a face has no material; a liquid's own colour.</summary>
	public Color Color => Info?.Color ?? Color.Clear;

	/// <summary>Whether it is a liquid, as water is: nothing stands on it, and blocks are placed through it.</summary>
	public bool IsLiquid => Info?.Liquid ?? false;

	/// <summary>Its shape: a cube, or a block of parts such as a slab, a stair, a fence or a door.</summary>
	public BlockShape Shape => Info?.Shape ?? BlockShape.Cube;

	/// <summary>Whether a player can use it: a door, a trap door, a gate, a lever, a button, or a block marked usable.</summary>
	public bool IsUsable => Info?.Usable ?? false;

	/// <summary>How many seconds it takes to break by hand.</summary>
	public float Hardness => Info?.Hardness ?? 0f;

	/// <summary>Whether nothing breaks it.</summary>
	public bool Unbreakable => Info?.Unbreakable ?? false;

	/// <summary>The kind of tool that breaks it fast; null for none.</summary>
	public ToolKind? Tool => Info?.Tool;

	/// <summary>The lowest tier of its kind of tool that gets its drops; 0 for anything, a hand too.</summary>
	public int Tier => Info?.Tier ?? 0;

	/// <summary>What it drops when it is broken.</summary>
	public IReadOnlyList<BlockDrop> Drops => Info?.Drops ?? [];

	/// <summary>The sounds of breaking it, placing it and stepping on it; None where it has none.</summary>
	public Sound BreakSound => Info?.Sound(0) ?? Sound.None;

	public Sound PlaceSound => Info?.Sound(1) ?? Sound.None;

	public Sound StepSound => Info?.Sound(2) ?? Sound.None;

	/// <summary>
	/// A block by project path or GUID, for example "Blocks/Stone.block". Logs an error and returns None when the
	/// project has no such block.
	/// </summary>
	public static Block Load(string path)
	{
		if (string.IsNullOrEmpty(path))
		{
			return None;
		}

		var found = BlockInfos.Key(path);
		if (found == null || BlockInfos.Of(found) == null)
		{
			Log.Error($"The project has no block \"{path}\".");
			return None;
		}
		return new Block(found);
	}

	internal static Block FromKey(string key)
	{
		return string.IsNullOrEmpty(key) ? None : new Block(key);
	}

	/// <summary>The data asset its Data names, as a T: a game's own values for it. Null where it names none, or not a T.</summary>
	public T? Data<T>() where T : Definition
	{
		return Info?.Data as T;
	}

	/// <summary>
	/// How many seconds it takes to break holding <paramref name="held"/>, or by hand with nothing: its Hardness, less
	/// by as many times as a tool of its kind is fast. Infinity for one nothing breaks. A block whose Data is a
	/// <see cref="BlockRules"/> is asked instead.
	/// </summary>
	public float BreakTime(Item? held = null)
	{
		return Data<BlockRules>() is { } rules ? rules.BreakTime(this, held) : TimeByHardness(held);
	}

	/// <summary>Its break time by its own Hardness and Tool alone, whatever its Data says.</summary>
	public float TimeByHardness(Item? held = null)
	{
		if (Unbreakable)
		{
			return float.PositiveInfinity;
		}
		return Hardness / Math.Max(held?.SpeedOn(this) ?? 1f, 0.01f);
	}

	/// <summary>
	/// Whether breaking it holding <paramref name="held"/> gets its drops: always with no Tier, else only with a tool
	/// of its kind of that tier or higher.
	/// </summary>
	public bool DropsFor(Item? held = null)
	{
		return held?.Harvests(this) ?? Tier <= 0;
	}

	/// <summary>
	/// What it drops this time, broken holding <paramref name="held"/>: each of its Drops rolled, or nothing where
	/// the tool held does not get them. A block whose Data is a <see cref="BlockRules"/> may change them.
	/// </summary>
	public List<ItemStack> RollDrops(Item? held = null, Random? random = null)
	{
		var drops = new List<ItemStack>();
		if (DropsFor(held))
		{
			foreach (var drop in Drops)
			{
				if (drop.Roll(random ?? Random.Shared) is { IsEmpty: false } rolled)
				{
					drops.Add(rolled);
				}
			}
		}
		Data<BlockRules>()?.Drops(this, held, drops);
		return drops;
	}

	public static bool operator ==(Block left, Block right)
	{
		return left.Equals(right);
	}

	public static bool operator !=(Block left, Block right)
	{
		return !left.Equals(right);
	}

	public bool Equals(Block other)
	{
		return Key == other.Key;
	}

	public override bool Equals(object? other)
	{
		return other is Block block && Equals(block);
	}

	public override int GetHashCode()
	{
		return Key.GetHashCode();
	}

	public override string ToString()
	{
		return IsValid ? Name : "Block(none)";
	}
}
