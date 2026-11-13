using System;
using System.Collections.Generic;
using System.Numerics;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// Items lying in the world. A tree breaks into what its blocks drop where it lands, and a game drops what it likes:
/// each falls onto the ground and rests there, drawn as a small block of what it places, until it is taken up or has
/// lain a while. An <see cref="ItemPickup"/> on a player takes up those in its reach by itself.
/// <code>
/// // Drop what a chest held where it stood.
/// foreach (var stack in chest.Stacks)
/// {
///     ItemDrops.Drop(stack, chest.Position + Vector3.UnitY);
/// }
///
/// // Take up everything within two metres.
/// foreach (var stack in ItemDrops.Collect(player.WorldPosition, 2f))
/// {
///     inventory.Add(stack);
/// }
/// </code>
/// </summary>
public static unsafe class ItemDrops
{
	/// <summary>
	/// Drops <paramref name="count"/> of <paramref name="item"/> at <paramref name="at"/>, thrown at
	/// <paramref name="velocity"/> metres a second: the drop, or one that is not valid where nothing was dropped.
	/// </summary>
	public static ItemDrop Drop(Item item, int count, Vector3 at, Vector3 velocity = default)
	{
		if (!Native.IsConnected || item == null || count <= 0 || string.IsNullOrEmpty(item.Guid))
		{
			return default;
		}
		var bytes = System.Text.Encoding.UTF8.GetBytes(item.Guid);
		fixed (byte* named = bytes)
		{
			return new ItemDrop(Native.Api->ItemDrop(named, bytes.Length, count, &at, &velocity));
		}
	}

	/// <summary>Drops a stack at <paramref name="at"/>, thrown at <paramref name="velocity"/>.</summary>
	public static ItemDrop Drop(ItemStack stack, Vector3 at, Vector3 velocity = default)
	{
		return stack.IsEmpty ? default : Drop(stack.Item!, stack.Count, at, velocity);
	}

	/// <summary>The items lying within <paramref name="radius"/> metres of <paramref name="at"/> that can be taken up, nearest first.</summary>
	public static ItemDrop[] Near(Vector3 at, float radius)
	{
		if (!Native.IsConnected)
		{
			return [];
		}
		var found = new ulong[64];
		int count;
		fixed (ulong* written = found)
		{
			count = Native.Api->ItemDropsNear(&at, radius, written, found.Length);
		}
		var drops = new ItemDrop[Math.Min(count, found.Length)];
		for (var each = 0; each < drops.Length; each++)
		{
			drops[each] = new ItemDrop(found[each]);
		}
		return drops;
	}

	/// <summary>Takes up <paramref name="drop"/>: what it was, or an empty stack where it was gone already.</summary>
	public static ItemStack Take(ItemDrop drop)
	{
		var item = drop.Read(out var count, out _, take: true);
		return item == null ? ItemStack.Empty : new ItemStack(Definition.Load<Item>(item), count);
	}

	/// <summary>Takes up every item lying within <paramref name="radius"/> metres of <paramref name="at"/>: what they were.</summary>
	public static List<ItemStack> Collect(Vector3 at, float radius)
	{
		var taken = new List<ItemStack>();
		foreach (var drop in Near(at, radius))
		{
			var stack = Take(drop);
			if (!stack.IsEmpty)
			{
				taken.Add(stack);
			}
		}
		return taken;
	}
}
