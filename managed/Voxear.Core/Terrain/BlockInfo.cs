using System.Collections.Generic;
using System.Text.Json;

namespace Voxear;

/// <summary>
/// What a .block file says, as scripts read it: its look and its rules. What it names (its tool kind, the items it
/// drops, its sounds, its data) is loaded the first time it is asked for.
/// </summary>
internal sealed class BlockInfo
{
	private readonly string? tool;
	private readonly string? data;
	private readonly string?[] sounds = new string?[3];
	private readonly List<(string? Item, int Low, int High, float Chance)> dropped = [];
	private ToolKind? toolKind;
	private Definition? dataAsset;
	private List<BlockDrop>? drops;
	private readonly Sound?[] loaded = new Sound?[3];

	/// <summary>A block as its values, for one made in code: a test's, with what it names given as it is.</summary>
	public BlockInfo(string name, float hardness = 1f, ToolKind? tool = null, int tier = 0, List<BlockDrop>? drops = null)
	{
		(Name, Hardness, Tier) = (name, hardness, tier);
		(toolKind, this.drops) = (tool, drops ?? []);
	}

	public BlockInfo(JsonElement json)
	{
		Name = json.GetProperty("name").GetString() ?? string.Empty;
		var color = json.GetProperty("color");
		Color = new Color(color[0].GetSingle(), color[1].GetSingle(), color[2].GetSingle());
		Liquid = json.GetProperty("liquid").GetString() is { } liquid && liquid != "none";
		Hardness = json.GetProperty("hardness").GetSingle();
		Unbreakable = json.GetProperty("unbreakable").GetBoolean();
		Tier = json.GetProperty("tier").GetInt32();
		tool = json.GetProperty("tool").GetString();
		data = json.GetProperty("data").GetString();
		var named = json.GetProperty("sounds");
		sounds = [named.GetProperty("break").GetString(), named.GetProperty("place").GetString(), named.GetProperty("step").GetString()];
		Shape = json.TryGetProperty("shape", out var shape) ? (BlockShape)shape.GetInt32() : BlockShape.Cube;
		Usable = json.TryGetProperty("usable", out var usable) && usable.GetBoolean();
		foreach (var drop in json.GetProperty("drops").EnumerateArray())
		{
			var count = drop.GetProperty("count");
			dropped.Add((drop.GetProperty("item").GetString(), count[0].GetInt32(), count[1].GetInt32(), drop.GetProperty("chance").GetSingle()));
		}
	}

	public string Name { get; }

	public Color Color { get; init; } = Color.Grey;

	public bool Liquid { get; init; }

	public BlockShape Shape { get; init; }

	public bool Usable { get; init; }

	public float Hardness { get; }

	public bool Unbreakable { get; init; }

	public int Tier { get; }

	public ToolKind? Tool => toolKind ??= string.IsNullOrEmpty(tool) ? null : Definition.Load<ToolKind>(tool);

	public Definition? Data => dataAsset ??= string.IsNullOrEmpty(data) ? null : Definition.Load<Definition>(data);

	public IReadOnlyList<BlockDrop> Drops
	{
		get
		{
			if (drops != null)
			{
				return drops;
			}
			drops = [];
			foreach (var (item, low, high, chance) in dropped)
			{
				if (!string.IsNullOrEmpty(item) && Definition.Load<Item>(item) is { } found)
				{
					drops.Add(new BlockDrop(found, low, high, chance));
				}
			}
			return drops;
		}
	}

	/// <summary>Its break, place or step sound, by 0, 1 or 2: None where it names none.</summary>
	public Sound Sound(int which)
	{
		return loaded[which] ??= string.IsNullOrEmpty(sounds[which]) ? Voxear.Sound.None : Voxear.Sound.Load(sounds[which]!);
	}
}
