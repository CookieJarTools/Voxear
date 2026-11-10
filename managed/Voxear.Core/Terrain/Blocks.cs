using System;
using System.Numerics;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// The level's worlds of blocks: which block is at a point, where a ray meets one, and changes to them. A block's
/// kind is its number in its world's list, from 1; 0 is air. Each change reaches every Block World it falls in, and
/// physics and drawing follow it at once.
/// <code>
/// // Break the block the player looks at, or put stone against it.
/// if (Blocks.Raycast(camera.Position, camera.Forward, out var hit, 8f))
/// {
///     if (Input.GetButtonDown("Fire")) Blocks.Break(hit);
///     if (Input.GetButtonDown("Use")) Blocks.Place(hit, Stone);
/// }
/// </code>
/// A save of the scene keeps what scripts changed, as only the chunks that differ from the world as its level built
/// it, and <see cref="CaptureEdits"/> gives them to a player joining a game.
/// </summary>
public static unsafe class Blocks
{
	/// <summary>Air: no block.</summary>
	public const ushort Air = 0;

	/// <summary>The block at <paramref name="position"/> in the first Block World with one there; air everywhere else.</summary>
	public static ushort At(Vector3 position)
	{
		return Native.IsConnected ? (ushort)Native.Api->BlockAt(&position, null) : Air;
	}

	/// <summary>Sets the block at <paramref name="position"/> to <paramref name="block"/>; whether it changed.</summary>
	public static bool Set(Vector3 position, ushort block)
	{
		return Change(0, position, Vector3.Zero, block, 0) > 0;
	}

	/// <summary>Sets every block in the box between two corners to <paramref name="block"/>; how many changed.</summary>
	public static int Fill(Vector3 from, Vector3 to, ushort block)
	{
		return Change(1, from, to, block, 0);
	}

	/// <summary>Sets every block whose middle is within <paramref name="radius"/> metres of a point; how many changed.</summary>
	public static int Ball(Vector3 middle, float radius, ushort block)
	{
		return Change(2, middle, new Vector3(radius, 0f, 0f), block, 0);
	}

	/// <summary>Takes away the blocks within <paramref name="radius"/> metres of a point, as a blast does; how many.</summary>
	public static int Break(Vector3 middle, float radius)
	{
		return Change(2, middle, new Vector3(radius, 0f, 0f), Air, 2);
	}

	/// <summary>Takes away the block a ray met; whether it was there to take.</summary>
	public static bool Break(in BlockHit hit)
	{
		return Change(0, hit.Middle, Vector3.Zero, Air, 2) > 0;
	}

	/// <summary>Puts <paramref name="block"/> against the face a ray met, where there is air; whether it went in.</summary>
	public static bool Place(in BlockHit hit, ushort block)
	{
		return Change(0, hit.Before, Vector3.Zero, block, 1) > 0;
	}

	/// <summary>
	/// The first block a ray from <paramref name="origin"/> along <paramref name="direction"/> meets within
	/// <paramref name="maxDistance"/> metres, in any Block World.
	/// </summary>
	public static bool Raycast(Vector3 origin, Vector3 direction, out BlockHit hit, float maxDistance = 1000f)
	{
		hit = default;
		if (!Native.IsConnected)
		{
			return false;
		}

		BlockHitData found;
		if (Native.Api->BlockRaycast(&origin, &direction, maxDistance, &found) == 0)
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

		fixed (byte* from = edits)
		{
			return Native.Api->BlockEdits(1, from, edits.Length, null, 0) >= 0;
		}
	}

	private static int Change(int shape, Vector3 a, Vector3 b, ushort block, int only)
	{
		return Native.IsConnected ? Native.Api->BlockSet(shape, &a, &b, block, only) : 0;
	}
}
