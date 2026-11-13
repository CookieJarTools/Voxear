using System.Numerics;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// What a placed block keeps of itself: which way it faces, which half of its block it is in, whether it is open and
/// whether it is on, and which side a door turns on. Each shape keeps only what it uses, so a fence's state is always
/// the first. It is part of the block's number in its world, so saved changes stay as small as they were.
/// <code>
/// // A stair facing east, upside down.
/// Blocks.Set(at, Stairs, new BlockState(BlockFacing.East, BlockHalf.Upper));
///
/// // Open the door the player looks at, without a sound of its own.
/// var state = Blocks.StateAt(hit.Middle);
/// Blocks.SetState(hit.Middle, state with { Open = true });
/// </code>
/// </summary>
public readonly record struct BlockState(
	BlockFacing Facing = BlockFacing.North,
	BlockHalf Half = BlockHalf.Lower,
	bool Open = false,
	bool On = false,
	DoorHinge Hinge = DoorHinge.Left)
{
	/// <summary>As one byte, as the engine keeps it.</summary>
	public byte ToByte()
	{
		var facing = (int)Facing & 7;
		var upper = Half == BlockHalf.Upper ? 8 : 0;
		var open = Open ? 16 : 0;
		var on = On ? 32 : 0;
		var right = Hinge == DoorHinge.Right ? 64 : 0;
		return (byte)(facing | upper | open | on | right);
	}

	/// <summary>A state from the byte the engine keeps.</summary>
	public static BlockState FromByte(int value)
	{
		var facing = (BlockFacing)((value & 7) % 6);
		var half = (value & 8) != 0 ? BlockHalf.Upper : BlockHalf.Lower;
		var hinge = (value & 64) != 0 ? DoorHinge.Right : DoorHinge.Left;
		return new BlockState(facing, half, (value & 16) != 0, (value & 32) != 0, hinge);
	}

	/// <summary>
	/// The state a block of <paramref name="shape"/> is put down in against the face a ray met, by someone looking
	/// <paramref name="looking"/>: a stair rising away from them, a slab lying on the face, a door's hinge on the side
	/// of the face it was put nearer, a lever standing out of the face.
	/// </summary>
	public static unsafe BlockState Placed(BlockShape shape, in BlockHit hit, Vector3 looking)
	{
		if (!Native.IsConnected)
		{
			return default;
		}
		var corner = hit.Before - new Vector3(hit.BlockSize * 0.5f);
		var within = (hit.Point - corner) / hit.BlockSize;
		var normal = hit.Normal;
		return FromByte(Native.Api->BlockPlacedState((int)shape, &within, &normal, &looking));
	}
}
