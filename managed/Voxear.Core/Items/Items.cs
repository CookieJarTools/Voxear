using System.Collections.Generic;

namespace Voxear;

/// <summary>The project's items, and which of them places a block or holds a liquid.</summary>
public static class Items
{
	private static IReadOnlyList<Item>? all;

	/// <summary>Every item of the project, by path. Read once, and again after <see cref="Refresh"/>.</summary>
	public static IReadOnlyList<Item> All => all ??= Definition.All<Item>();

	/// <summary>Reads the project's items again, as after one was made while the game runs.</summary>
	public static void Refresh()
	{
		all = null;
	}

	/// <summary>The item that places <paramref name="block"/> and leaves nothing behind: the block as it is carried. Null where none does.</summary>
	public static Item? Placing(Block block)
	{
		return Placing(All, block);
	}

	/// <summary>
	/// The item <paramref name="empty"/> becomes once it has taken up the liquid <paramref name="liquid"/>: the one
	/// that places it and leaves <paramref name="empty"/> behind, as a bucket of water does a bucket. Null where
	/// nothing holds that liquid in it.
	/// </summary>
	public static Item? Filled(Item empty, Block liquid)
	{
		return Filled(All, empty, liquid);
	}

	internal static Item? Placing(IReadOnlyList<Item> items, Block block)
	{
		foreach (var item in items)
		{
			if (block.IsValid && item.Places == block && item.Leaves == null)
			{
				return item;
			}
		}
		return null;
	}

	internal static Item? Filled(IReadOnlyList<Item> items, Item empty, Block liquid)
	{
		foreach (var item in items)
		{
			if (liquid.IsValid && item.Places == liquid && item.Leaves == empty)
			{
				return item;
			}
		}
		return null;
	}
}
