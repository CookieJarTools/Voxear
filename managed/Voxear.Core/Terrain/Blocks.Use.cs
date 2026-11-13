using System.Collections.Generic;
using System.Numerics;
using System.Text;
using Voxear.Interop;

namespace Voxear;

public static unsafe partial class Blocks
{
	/// <summary>A block is about to be used: a listener may set Cancel to leave it as it is.</summary>
	public static event System.Action<BlockUse>? Using;

	/// <summary>A block was used: a door opened or shut, a lever turned, a button pressed, or a usable block said so.</summary>
	public static event System.Action<BlockUse>? Used;

	/// <summary>
	/// Uses the block at <paramref name="position"/>, as a player does with the Use action: a door opens or shuts,
	/// with the door beside it hung on the other side; a lever turns over and opens or shuts what it is linked to; a
	/// button turns on for a moment. A block marked usable says it was used, for a script to answer in Used. Its
	/// block's <see cref="BlockRules.CanUse"/> and the Using event may stop it. Whether it was used.
	/// <code>
	/// if (use.Action.Pressed &amp;&amp; Blocks.Raycast(eye.Position, eye.Forward, out var hit, 4f))
	/// {
	///     Blocks.Use(hit.Middle, Obj);
	/// }
	/// </code>
	/// </summary>
	public static bool Use(Vector3 position, Obj user = default)
	{
		if (!Native.IsConnected)
		{
			return false;
		}
		var block = DefinitionAt(position);
		var use = new BlockUse(position, block, user);
		var rules = block.Data<BlockRules>();
		if (rules != null && !rules.CanUse(use))
		{
			return false;
		}
		Using?.Invoke(use);
		if (use.Cancel)
		{
			return false;
		}
		var done = Native.Api->BlockUse(&position);
		if (done == 0)
		{
			return false;
		}
		// A world may have numbered the state it went to.
		BlockInfos.Renumbered();
		use.Kind = (BlockUseKind)(done & 15);
		use.Now = (done & 16) != 0;
		rules?.OnUsed(use);
		Used?.Invoke(use);
		return true;
	}

	/// <summary>Uses the block a ray met (see <see cref="Use(Vector3, Obj)"/>).</summary>
	public static bool Use(in BlockHit hit, Obj user = default)
	{
		return Use(hit.Middle, user);
	}

	/// <summary>The state the block at <paramref name="position"/> is placed in; the first state for air.</summary>
	public static BlockState StateAt(Vector3 position)
	{
		if (!Native.IsConnected)
		{
			return default;
		}
		var state = Native.Api->BlockState(&position);
		return state < 0 ? default : BlockState.FromByte(state);
	}

	/// <summary>
	/// Puts the block at <paramref name="position"/> in <paramref name="state"/>, the same block. Where only whether it
	/// is open (or on) changes, it moves there by its motion, as using it would, a door with its pair and a lever with
	/// what it drives; with <paramref name="animate"/> false, at once. Whether anything changed.
	/// </summary>
	public static bool SetState(Vector3 position, BlockState state, bool animate = true)
	{
		if (!Native.IsConnected)
		{
			return false;
		}
		var changed = Native.Api->BlockSetState(&position, state.ToByte(), animate ? 1 : 0) > 0;
		BlockInfos.Renumbered();
		return changed;
	}

	/// <summary>Sets the block at <paramref name="position"/> to <paramref name="block"/> placed in <paramref name="state"/>; whether it changed.</summary>
	public static bool Set(Vector3 position, Block block, BlockState state)
	{
		if (!Native.IsConnected || !block.IsValid)
		{
			return false;
		}
		var key = Encoding.UTF8.GetBytes(block.Key);
		int changed;
		fixed (byte* named = key)
		{
			changed = Native.Api->BlockSetPlaced(&position, named, key.Length, state.ToByte());
		}
		BlockInfos.Renumbered();
		return changed > 0;
	}

	/// <summary>
	/// Puts <paramref name="block"/> against the face a ray met, where there is air or a liquid, turned as someone
	/// looking <paramref name="looking"/> puts it down (see <see cref="BlockState.Placed"/>): a stair rising away from
	/// them, a door with its upper block over it. Whether it went in.
	/// </summary>
	public static bool Place(in BlockHit hit, Block block, Vector3 looking)
	{
		var here = At(hit.Before);
		if (here != Air && !DefinitionAt(hit.Before).IsLiquid)
		{
			return false;
		}
		return Set(hit.Before, block, BlockState.Placed(block.Shape, hit, looking));
	}

	/// <summary>
	/// Links the lever or button at <paramref name="from"/> to the block at <paramref name="to"/>, in the same Block
	/// World and within 64 blocks: turned on, it opens a door, a trap door or a gate there, and shuts it turned off.
	/// Links are saved with the world's changes. Whether it was linked now.
	/// </summary>
	public static bool Link(Vector3 from, Vector3 to)
	{
		return Native.IsConnected && Native.Api->BlockLink(0, &from, &to) > 0;
	}

	/// <summary>Takes away the link from <paramref name="from"/> to <paramref name="to"/>, or every link from it where <paramref name="to"/> is null.</summary>
	public static bool Unlink(Vector3 from, Vector3? to = null)
	{
		if (!Native.IsConnected)
		{
			return false;
		}
		if (to is { } end)
		{
			return Native.Api->BlockLink(1, &from, &end) > 0;
		}
		return Native.Api->BlockLink(2, &from, null) > 0;
	}

	/// <summary>The middles of the blocks the lever or button at <paramref name="from"/> drives.</summary>
	public static IReadOnlyList<Vector3> LinksFrom(Vector3 from)
	{
		if (!Native.IsConnected)
		{
			return [];
		}
		var count = Native.Api->BlockLinks(&from, null, 0);
		var found = new Vector3[System.Math.Max(count, 0)];
		fixed (Vector3* into = found)
		{
			Native.Api->BlockLinks(&from, into, found.Length);
		}
		return found;
	}
}
