using System.Collections.Generic;

namespace Voxear;

/// <summary>
/// A block being broken by <see cref="Blocks.Mine(in BlockHit, Item)"/>, as <see cref="Blocks.Mining"/> hears it
/// before it goes and <see cref="Blocks.Mined"/> after: where it is, which block, what broke it and what it drops.
/// A listener to Mining may change the drops, or set Cancel to leave the block where it is.
/// </summary>
public sealed class BlockMining(BlockHit hit, Block block, Item? held, List<ItemStack> drops)
{
	public BlockHit Hit { get; } = hit;

	public Block Block { get; } = block;

	public Item? Held { get; } = held;

	public List<ItemStack> Drops { get; } = drops;

	public bool Cancel { get; set; }
}
