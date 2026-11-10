namespace Voxear;

/// <summary>
/// A block put down by <see cref="Blocks.Place(in BlockHit, Item)"/>, as <see cref="Blocks.Placed"/> hears it: the
/// face it went against, which block, and the item that placed it.
/// </summary>
public readonly struct BlockPlaced(BlockHit hit, Block block, Item? item)
{
	public BlockHit Hit { get; } = hit;

	public Block Block { get; } = block;

	public Item? Item { get; } = item;
}
