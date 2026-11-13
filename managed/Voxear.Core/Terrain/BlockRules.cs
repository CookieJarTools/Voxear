using System.Collections.Generic;

namespace Voxear;

/// <summary>
/// Rules of a game's own for a block. Derive a data asset from it, give it to a block as its Data, and the block asks
/// it how long it takes to break and what it drops, and tells it when it is broken or placed. Each has the block's
/// own answer to start from, so override only what differs.
/// <code>
/// [CreateAsset("Blocks/Ore Rules")]
/// public sealed class OreRules : BlockRules
/// {
///     public Item? Gem;
///
///     public override void Drops(Block block, Item? held, List&lt;ItemStack&gt; drops)
///     {
///         if (Gem != null &amp;&amp; Random.Shared.NextSingle() &lt; 0.05f) drops.Add(new ItemStack(Gem));
///     }
/// }
/// </code>
/// </summary>
public abstract class BlockRules : Definition
{
	/// <summary>How many seconds <paramref name="block"/> takes to break holding <paramref name="held"/>.</summary>
	public virtual float BreakTime(Block block, Item? held)
	{
		return block.TimeByHardness(held);
	}

	/// <summary>Changes what <paramref name="block"/> drops this time: <paramref name="drops"/> are its own, rolled already.</summary>
	public virtual void Drops(Block block, Item? held, List<ItemStack> drops)
	{
	}

	/// <summary>Runs once the block a ray met was broken by <see cref="Blocks.Mine(in BlockHit, Item)"/>.</summary>
	public virtual void OnMined(in BlockHit hit, Item? held)
	{
	}

	/// <summary>Runs once the block was put against the face a ray met by <see cref="Blocks.Place(in BlockHit, Item)"/>.</summary>
	public virtual void OnPlaced(in BlockHit hit, Item? item)
	{
	}

	/// <summary>
	/// Whether the block may be used now, by <see cref="BlockUse.User"/>: false keeps a door shut, as a lock does.
	/// <code>
	/// public override bool CanUse(BlockUse use) =&gt; use.User.Get&lt;Keyring&gt;()?.Has(Key) == true;
	/// </code>
	/// </summary>
	public virtual bool CanUse(BlockUse use)
	{
		return true;
	}

	/// <summary>Runs once the block was used by <see cref="Blocks.Use(System.Numerics.Vector3, Obj)"/>.</summary>
	public virtual void OnUsed(BlockUse use)
	{
	}
}
