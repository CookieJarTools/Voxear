using System;

namespace Voxear;

/// <summary>
/// Something a block drops when it is broken: the item, how many of it, from the fewest to the most, and its chance
/// of dropping at all, from 0 to 1.
/// </summary>
public readonly struct BlockDrop(Item item, int fewest = 1, int most = 1, float chance = 1f)
{
	public Item Item { get; } = item;

	public int Fewest { get; } = Math.Max(fewest, 0);

	public int Most { get; } = Math.Max(most, Math.Max(fewest, 0));

	public float Chance { get; } = Math.Clamp(chance, 0f, 1f);

	/// <summary>What drops this time: so many of its item, or nothing.</summary>
	public ItemStack Roll(Random random)
	{
		if (Chance < 1f && random.NextSingle() >= Chance)
		{
			return ItemStack.Empty;
		}
		return new ItemStack(Item, random.Next(Fewest, Most + 1));
	}
}
