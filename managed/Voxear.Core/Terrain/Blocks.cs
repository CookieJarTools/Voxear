using System;
using System.Collections.Generic;
using System.Numerics;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// The level's worlds of blocks: which block is at a point, where a ray meets one, and changes to them. Each change
/// reaches every Block World it falls in, and physics and drawing follow it at once.
/// <code>
/// // Break the block the player looks at, or put stone against it.
/// public Block Stone;
/// ...
/// if (Blocks.Raycast(camera.Position, camera.Forward, out var hit, 8f))
/// {
///     if (Input.Pressed(fire)) Blocks.Break(hit);
///     if (Input.Pressed(use)) Blocks.Place(hit, Stone);
/// }
/// </code>
/// A block is named by its <see cref="Block"/>, a .block file, which is the same block in every world: any world can
/// hold any block of the project, and <see cref="All"/> is every one of them. A world numbers a block the first time
/// it holds one, from 1, with 0 for air, and a block can be named by that number too. A number is that world's own,
/// and for a block the world first held while the game ran, this machine's own, so send a Block to another machine,
/// never its number.
/// <para>
/// With items, the blocks' own rules apply: <see cref="Mine(in BlockHit, Item)"/> breaks a block and gives what it
/// drops to the tool held, <see cref="Place(in BlockHit, Item)"/> puts down the block an item places, and
/// <see cref="BlockMiner"/> counts the time a block takes to break.
/// </para>
/// A save of the scene keeps what scripts changed, as only the chunks that differ from the world as its level built
/// it, and <see cref="CaptureEdits"/> gives them to a player joining a game.
/// </summary>
public static unsafe class Blocks
{
	/// <summary>Air: no block.</summary>
	public const ushort Air = 0;

	/// <summary>Which blocks a change reaches: all, only air, only blocks, air and liquids, or only liquids at rest.</summary>
	private const int Everything = 0;
	private const int AirOnly = 1;
	private const int BlocksOnly = 2;
	private const int AirAndLiquids = 3;
	private const int LiquidSources = 4;

	/// <summary>
	/// A block is about to be broken by <see cref="Mine(in BlockHit, Item)"/>: a listener may change what it drops, or
	/// set Cancel to leave it where it is.
	/// </summary>
	public static event Action<BlockMining>? Mining;

	/// <summary>A block was broken by <see cref="Mine(in BlockHit, Item)"/>, and dropped what its Drops say.</summary>
	public static event Action<BlockMining>? Mined;

	/// <summary>A block was put down by <see cref="Place(in BlockHit, Item)"/>.</summary>
	public static event Action<BlockPlaced>? Placed;

	/// <summary>
	/// Every block of the project, in the order of their paths: any world can hold any of them.
	/// <code>
	/// foreach (var block in Blocks.All)
	/// {
	///     Log.Info($"{block.Name}: {block.Hardness}s");
	/// }
	/// </code>
	/// </summary>
	public static IReadOnlyList<Block> All => BlockInfos.All();

	/// <summary>The block at <paramref name="position"/> in the first Block World with one there; None for air, and for a kind a world keeps itself.</summary>
	public static Block DefinitionAt(Vector3 position)
	{
		if (!Native.IsConnected)
		{
			return Block.None;
		}
		ulong obj = 0;
		var number = (ushort)Native.Api->BlockAt(&position, &obj);
		return BlockInfos.Numbered(new Obj(obj), number);
	}

	/// <summary>The block at <paramref name="position"/> in the first Block World with one there; air everywhere else.</summary>
	public static ushort At(Vector3 position)
	{
		return Native.IsConnected ? (ushort)Native.Api->BlockAt(&position, null) : Air;
	}

	/// <summary>Sets the block at <paramref name="position"/> to <paramref name="block"/>; whether it changed.</summary>
	public static bool Set(Vector3 position, ushort block)
	{
		return Change(0, position, Vector3.Zero, block, Everything) > 0;
	}

	/// <summary>Sets the block at <paramref name="position"/> to <paramref name="block"/>; whether it changed.</summary>
	public static bool Set(Vector3 position, Block block)
	{
		return Change(0, position, Vector3.Zero, block, Everything) > 0;
	}

	/// <summary>Sets every block in the box between two corners to <paramref name="block"/>; how many changed.</summary>
	public static int Fill(Vector3 from, Vector3 to, ushort block)
	{
		return Change(1, from, to, block, Everything);
	}

	/// <summary>Sets every block in the box between two corners to <paramref name="block"/>; how many changed.</summary>
	public static int Fill(Vector3 from, Vector3 to, Block block)
	{
		return Change(1, from, to, block, Everything);
	}

	/// <summary>Sets every block whose middle is within <paramref name="radius"/> metres of a point; how many changed.</summary>
	public static int Ball(Vector3 middle, float radius, ushort block)
	{
		return Change(2, middle, new Vector3(radius, 0f, 0f), block, Everything);
	}

	/// <summary>Sets every block whose middle is within <paramref name="radius"/> metres of a point; how many changed.</summary>
	public static int Ball(Vector3 middle, float radius, Block block)
	{
		return Change(2, middle, new Vector3(radius, 0f, 0f), block, Everything);
	}

	/// <summary>Takes away the blocks within <paramref name="radius"/> metres of a point, as a blast does; how many.</summary>
	public static int Break(Vector3 middle, float radius)
	{
		return Change(2, middle, new Vector3(radius, 0f, 0f), Air, BlocksOnly);
	}

	/// <summary>Takes away the block a ray met, whatever its rules; whether it was there to take.</summary>
	public static bool Break(in BlockHit hit)
	{
		return Change(0, hit.Middle, Vector3.Zero, Air, BlocksOnly) > 0;
	}

	/// <summary>
	/// Puts <paramref name="block"/> against the face a ray met, where there is air or a liquid, which it takes the
	/// place of; whether it went in.
	/// </summary>
	public static bool Place(in BlockHit hit, ushort block)
	{
		return Change(0, hit.Before, Vector3.Zero, block, AirAndLiquids) > 0;
	}

	/// <summary>
	/// Puts <paramref name="block"/> against the face a ray met, where there is air or a liquid, which it takes the
	/// place of; whether it went in.
	/// </summary>
	public static bool Place(in BlockHit hit, Block block)
	{
		return Change(0, hit.Before, Vector3.Zero, block, AirAndLiquids) > 0;
	}

	/// <summary>
	/// Puts down the block <paramref name="item"/> places against the face a ray met, with its sound; false, and
	/// nothing done, for an item that places none, or where there is no room. Taking one of the item from whoever
	/// holds it, and giving them the item it <see cref="Item.Leaves"/>, is the caller's to do.
	/// </summary>
	public static bool Place(in BlockHit hit, Item item)
	{
		var block = item.Places;
		if (!block.IsValid || !Place(hit, block))
		{
			return false;
		}
		Sound(block.PlaceSound, hit.Before);
		block.Data<BlockRules>()?.OnPlaced(hit, item);
		Placed?.Invoke(new BlockPlaced(hit, block, item));
		return true;
	}

	/// <summary>
	/// Breaks the block a ray met by its rules, at once, and gives what it drops to the tool <paramref name="held"/>:
	/// nothing where the tool does not get its drops, and nothing, the block left there, where nothing breaks it. A
	/// kind a world keeps itself has no rules: it breaks and drops nothing. Use a <see cref="BlockMiner"/> to take the
	/// time the block takes.
	/// </summary>
	public static ItemStack[] Mine(in BlockHit hit, Item? held = null)
	{
		Mine(hit, held, out var drops);
		return drops;
	}

	/// <summary>As <see cref="Mine(in BlockHit, Item)"/>, saying whether the block broke.</summary>
	public static bool Mine(in BlockHit hit, Item? held, out ItemStack[] drops)
	{
		drops = [];
		var block = hit.Definition;
		if (block.Unbreakable)
		{
			return false;
		}
		var mining = new BlockMining(hit, block, held, block.RollDrops(held));
		Mining?.Invoke(mining);
		if (mining.Cancel || !Break(hit))
		{
			return false;
		}
		drops = [.. mining.Drops];
		Sound(block.BreakSound, hit.Middle);
		block.Data<BlockRules>()?.OnMined(hit, held);
		Mined?.Invoke(mining);
		return true;
	}

	/// <summary>
	/// Takes up the liquid a ray met into the item <paramref name="held"/>, as a bucket takes water: the liquid's
	/// block goes, and the item that places that liquid and leaves <paramref name="held"/> behind is given back, to
	/// hold in its place. Null, and nothing done, where no item holds that liquid, or it is running rather than at
	/// rest. Cast the ray with liquids met, or it passes through them.
	/// </summary>
	public static Item? Scoop(in BlockHit hit, Item held)
	{
		var liquid = hit.Definition;
		if (!liquid.IsLiquid || Items.Filled(held, liquid) is not { } filled)
		{
			return null;
		}
		return Change(0, hit.Middle, Vector3.Zero, Air, LiquidSources) > 0 ? filled : null;
	}

	/// <summary>
	/// The first block a ray from <paramref name="origin"/> along <paramref name="direction"/> meets within
	/// <paramref name="maxDistance"/> metres, in any Block World. It passes through liquids, as a hand reaching into
	/// water does, unless <paramref name="liquids"/> says to meet them, as a bucket does.
	/// </summary>
	public static bool Raycast(Vector3 origin, Vector3 direction, out BlockHit hit, float maxDistance = 1000f, bool liquids = false)
	{
		hit = default;
		if (!Native.IsConnected)
		{
			return false;
		}

		BlockHitData found;
		if (Native.Api->BlockRaycast(&origin, &direction, maxDistance, liquids ? 1 : 0, &found) == 0)
		{
			return false;
		}

		hit = new BlockHit(
			new Obj(found.Obj),
			new Vector3(found.PointX, found.PointY, found.PointZ),
			new Vector3(found.NormalX, found.NormalY, found.NormalZ),
			found.Distance,
			new Vector3(found.MiddleX, found.MiddleY, found.MiddleZ),
			found.BlockSize,
			(ushort)found.Block);
		return true;
	}

	/// <summary>
	/// Every change scripts made to the level's worlds of blocks, as few bytes as say them. A save of the scene keeps
	/// them; a game sends them to a player who joins, who gives them to <see cref="ApplyEdits"/>.
	/// </summary>
	public static byte[] CaptureEdits()
	{
		if (!Native.IsConnected)
		{
			return [];
		}

		var length = Native.Api->BlockEdits(0, null, 0, null, 0);
		while (true)
		{
			var buffer = new byte[Math.Max(length, 0)];
			fixed (byte* into = buffer)
			{
				length = Native.Api->BlockEdits(0, null, 0, into, buffer.Length);
			}
			if (length <= buffer.Length)
			{
				return length == buffer.Length ? buffer : buffer[..length];
			}
		}
	}

	/// <summary>
	/// Puts each world of blocks <paramref name="edits"/> (from <see cref="CaptureEdits"/>) names back as its level
	/// built it, then makes its changes. Whether they were changes; worlds this level has not are left out.
	/// </summary>
	public static bool ApplyEdits(byte[] edits)
	{
		if (!Native.IsConnected || edits.Length == 0)
		{
			return Native.IsConnected;
		}

		// A world may number blocks it did not list, as the changes name them.
		BlockInfos.Renumbered();
		fixed (byte* from = edits)
		{
			return Native.Api->BlockEdits(1, from, edits.Length, null, 0) >= 0;
		}
	}

	private static int Change(int shape, Vector3 a, Vector3 b, ushort block, int only)
	{
		return Native.IsConnected ? Native.Api->BlockSet(shape, &a, &b, block, only) : 0;
	}

	/// <summary>As the change by number, the block numbered in each world it reaches as that world has it.</summary>
	private static int Change(int shape, Vector3 a, Vector3 b, Block block, int only)
	{
		if (!block.IsValid)
		{
			return 0;
		}
		var changed = NativeBlocks.Set(shape, a, b, block.Key, only);
		// A world that did not list the block has numbered it now.
		BlockInfos.Renumbered();
		return changed;
	}

	private static void Sound(Sound sound, Vector3 at)
	{
		if (sound.IsValid)
		{
			Audio.Play(sound, at);
		}
	}
}
