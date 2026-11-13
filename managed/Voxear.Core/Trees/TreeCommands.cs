using System.Linq;
using System.Numerics;

namespace Voxear;

/// <summary>
/// Trees and saplings from a development build's console, and so from a terminal through the editor (`vox editor call
/// game.console`): each takes a point in metres as "x,y,z".
/// </summary>
internal static class TreeCommands
{
	[Command("Chop the log at a point once")]
	private static string ChopLog(Vector3 at, Vector3 away)
	{
		return Trees.Chop(at, away).ToString();
	}

	[Command("Cut the log at a point through and fell its tree")]
	private static string FellTree(Vector3 at, Vector3 away)
	{
		return Trees.Fell(at, away) ? "felled" : "nothing came down";
	}

	[Command("Plant a sapling, by its block's path, on the ground under a point")]
	private static string PlantSapling(string block, Vector3 at)
	{
		return Saplings.Plant(Block.Load(block), at) ? "planted" : "it cannot take root there";
	}

	[Command("Grow the sapling at a point to its next stage")]
	private static string GrowSapling(Vector3 at)
	{
		return Saplings.Grow(at) ? "grew" : "it cannot grow now";
	}

	[Command("List the saplings growing")]
	private static string ListSaplings()
	{
		var all = Saplings.All;
		return all.Length == 0 ? "no saplings" : string.Join("\n", all.Select(s => $"{s.Position} {s.Grown:P0}"));
	}

	[Command("List the items lying near a point")]
	private static string ListDrops(Vector3 at, float radius)
	{
		var near = ItemDrops.Near(at, radius);
		return near.Length == 0 ? "nothing lying there" : string.Join("\n", near.Select(d => $"{d.Count} {d.Item?.Name} at {d.Position}"));
	}
}
