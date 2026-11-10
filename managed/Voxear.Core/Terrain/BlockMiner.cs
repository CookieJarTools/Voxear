using System.Numerics;

namespace Voxear;

/// <summary>
/// Counts how long a block has been dug at, and breaks it once that is as long as it takes. Keep one on whatever
/// digs, and call Dig every frame the dig button is held on the block a ray meets; call Stop when it is let go.
/// <code>
/// private readonly BlockMiner miner = new();
/// ...
/// if (Input.Held(dig) &amp;&amp; Blocks.Raycast(eye, forward, out var hit, 6f))
/// {
///     if (miner.Dig(hit, held, out var drops)) inventory.Add(drops);
/// }
/// else
/// {
///     miner.Stop();
/// }
/// crack.Value = miner.Progress;
/// </code>
/// </summary>
public sealed class BlockMiner
{
	private Obj world;
	private Vector3 middle;
	private Item? tool;
	private float dug;
	private float needed;

	/// <summary>How far through breaking the block it is on, from 0 to 1; 0 when it is on none.</summary>
	public float Progress => IsDigging && needed > 0f ? System.Math.Clamp(dug / needed, 0f, 1f) : 0f;

	/// <summary>Whether it is part way through a block.</summary>
	public bool IsDigging { get; private set; }

	/// <summary>The middle of the block it is on.</summary>
	public Vector3 Target => middle;

	/// <summary>
	/// Digs on at the block <paramref name="hit"/> met holding <paramref name="held"/>, for this frame or for
	/// <paramref name="seconds"/>. Another block, or another item held, starts again from nothing. True on the frame
	/// the block breaks, with what it dropped; a block nothing breaks never does.
	/// </summary>
	public bool Dig(in BlockHit hit, Item? held, out ItemStack[] drops, float seconds = -1f)
	{
		drops = [];
		if (!IsDigging || hit.Obj != world || hit.Middle != middle || held != tool)
		{
			(world, middle, tool, dug, IsDigging) = (hit.Obj, hit.Middle, held, 0f, true);
		}
		needed = hit.Definition.BreakTime(held);
		dug += seconds >= 0f ? seconds : Time.Delta;
		if (dug < needed)
		{
			return false;
		}
		Stop();
		return Blocks.Mine(hit, held, out drops);
	}

	/// <summary>Leaves the block it was on: the next Dig starts from nothing.</summary>
	public void Stop()
	{
		(IsDigging, dug, needed) = (false, 0f, 0f);
	}
}
