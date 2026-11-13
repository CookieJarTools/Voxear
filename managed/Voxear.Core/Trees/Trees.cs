using System;
using System.Collections.Generic;
using System.Numerics;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// Trees of blocks: a log is mined a slice at a time, and once one is cut through the tree over it topples and breaks
/// where it lands into what its blocks drop. <see cref="Blocks.Mine(in BlockHit, Item)"/> and a <see cref="BlockMiner"/>
/// do this by themselves, the tree falling away from whoever mined it; these are for a game's own axes.
/// <code>
/// // Fell a tree at once, away from the player.
/// if (Blocks.Raycast(eye, forward, out var hit, 5f) &amp;&amp; hit.Definition.Fells)
/// {
///     Trees.Fell(hit, forward);
/// }
///
/// Trees.Felled += tree => Log.Info($"A tree fell and broke into {tree.Drops.Count} drops");
/// </code>
/// </summary>
public static unsafe class Trees
{
	private const int ChopCall = 0;
	private const int FellCall = 1;

	/// <summary>
	/// Raised once a tree that came down has landed and broken into its drops. Take some up or drop more to change what
	/// it gave; what each block drops is its own, on its Inspector.
	/// </summary>
	public static event Action<TreeFelled>? Felled;

	/// <summary>The way away from the face a ray met, along the ground: the way a tree mined there falls.</summary>
	public static Vector3 Away(in BlockHit hit)
	{
		var away = new Vector3(-hit.Normal.X, 0f, -hit.Normal.Z);
		return away.LengthSquared() > 1e-6f ? Vector3.Normalize(away) : Vector3.UnitX;
	}

	/// <summary>
	/// How many more times the log a ray met is mined before it is cut through: 1 for its last slice, and for any block
	/// that breaks at once; 0 for none.
	/// </summary>
	public static int SlicesLeft(in BlockHit hit)
	{
		var block = hit.Definition;
		if (!block.IsValid)
		{
			return 0;
		}
		if (block.Shape != BlockShape.Log || block.Chops <= 1)
		{
			return 1;
		}
		var slice = Math.Max(16 / Math.Min(block.Chops, 8), 2);
		var across = Blocks.StateAt(hit.Middle).Across;
		return Math.Max((across - 4) / slice + 1, 1);
	}

	/// <summary>
	/// Mines the log a ray met once: a slice off it, or, its last, cut through, and the tree it held up comes down,
	/// falling <paramref name="away"/>. What happened; None where it is no log that chops.
	/// </summary>
	public static ChopResult Chop(in BlockHit hit, Vector3 away)
	{
		return Call(ChopCall, hit.Middle, away);
	}

	/// <summary>
	/// Cuts the log a ray met through at once, and brings down the tree it held up, falling <paramref name="away"/>:
	/// whether a tree came down.
	/// </summary>
	public static bool Fell(in BlockHit hit, Vector3 away)
	{
		return Fell(hit.Middle, away);
	}

	/// <summary>As <see cref="Chop(in BlockHit, Vector3)"/>, for the log at a point in metres.</summary>
	public static ChopResult Chop(Vector3 at, Vector3 away)
	{
		return Call(ChopCall, at, away);
	}

	/// <summary>As <see cref="Fell(in BlockHit, Vector3)"/>, for the log at a point in metres.</summary>
	public static bool Fell(Vector3 at, Vector3 away)
	{
		return Call(FellCall, at, away) == ChopResult.Felled;
	}

	private static ChopResult Call(int what, Vector3 at, Vector3 way)
	{
		if (!Native.IsConnected)
		{
			return ChopResult.None;
		}
		return Native.Api->TreeCall(what, &at, &way) switch
		{
			1 => ChopResult.Thinner,
			2 => ChopResult.Cut,
			3 => ChopResult.Felled,
			_ => ChopResult.None,
		};
	}

	/// <summary>Raises Felled for each tree that came down since last frame.</summary>
	internal static void Dispatch()
	{
		if (!Native.IsConnected || Felled == null)
		{
			return;
		}
		var trees = stackalloc TreeFelledData[16];
		var drops = new ulong[1024];
		int count;
		fixed (ulong* written = drops)
		{
			count = Native.Api->TreesFelled(trees, 16, written, drops.Length);
		}
		for (var at = 0; at < Math.Min(count, 16); at++)
		{
			var tree = trees[at];
			var lying = new List<ItemDrop>(tree.DropCount);
			for (var drop = 0; drop < tree.DropCount; drop++)
			{
				lying.Add(new ItemDrop(drops[tree.FirstDrop + drop]));
			}
			Felled?.Invoke(new TreeFelled(new Obj(tree.World), tree.Cut, tree.Away, tree.Landed, lying));
		}
	}
}
