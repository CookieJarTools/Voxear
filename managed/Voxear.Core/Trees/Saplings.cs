using System;
using System.Numerics;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// Saplings that grow into trees. A sapling is a block whose shape is Sapling and which names the plant it grows into:
/// it takes root on ground where there is room and light, grows through the plant's stages over its Grow time, and dies
/// where it waits too long for either. One takes root when an item that places it is put down
/// (<see cref="Blocks.Place(in BlockHit, Item)"/>), when a felled tree scatters its plant's saplings, and when a
/// sapling item lies a few seconds on the ground.
/// <code>
/// // Plant an oak where the player looks, and grow it a stage at once.
/// if (Blocks.Raycast(eye, forward, out var hit, 6f) &amp;&amp; Saplings.Plant(OakSapling, hit.Before))
/// {
///     Saplings.Grow(hit.Before);
/// }
/// </code>
/// </summary>
public static unsafe class Saplings
{
	private const int GrowCall = 2;

	/// <summary>
	/// Plants <paramref name="sapling"/> on the ground under <paramref name="at"/>, where it has room and enough light
	/// for its plant: whether it took root.
	/// </summary>
	public static bool Plant(Block sapling, Vector3 at)
	{
		if (!Native.IsConnected || !sapling.IsValid)
		{
			return false;
		}
		var bytes = System.Text.Encoding.UTF8.GetBytes(sapling.Key);
		fixed (byte* named = bytes)
		{
			return Native.Api->SaplingPlant(named, bytes.Length, &at) != 0;
		}
	}

	/// <summary>Grows the sapling at <paramref name="at"/> to its next stage now, where it has room: whether it grew.</summary>
	public static bool Grow(Vector3 at)
	{
		if (!Native.IsConnected)
		{
			return false;
		}
		var way = Vector3.Zero;
		return Native.Api->TreeCall(GrowCall, &at, &way) == 4;
	}

	/// <summary>Every sapling growing, in every Block World.</summary>
	public static SaplingInfo[] All
	{
		get
		{
			if (!Native.IsConnected)
			{
				return [];
			}
			var count = Native.Api->Saplings(null, 0);
			var found = new Vector4[Math.Max(count, 0)];
			fixed (Vector4* written = found)
			{
				count = Native.Api->Saplings(written, found.Length);
			}
			var saplings = new SaplingInfo[Math.Min(count, found.Length)];
			for (var at = 0; at < saplings.Length; at++)
			{
				saplings[at] = new SaplingInfo(new Vector3(found[at].X, found[at].Y, found[at].Z), found[at].W);
			}
			return saplings;
		}
	}
}
