using System;
using System.Numerics;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// An item lying in the world, as a tree breaks into where it lands or <see cref="ItemDrops.Drop(Item, int, System.Numerics.Vector3, System.Numerics.Vector3)"/> drops: which item,
/// how many, and where. It goes once it is taken up, or after lying a while.
/// </summary>
public readonly unsafe struct ItemDrop(ulong id) : IEquatable<ItemDrop>
{
	internal ulong Id { get; } = id;

	/// <summary>Whether it still lies there: false once taken up, or gone.</summary>
	public bool IsValid => Read(out _, out _) != null;

	/// <summary>Its item; null once it is gone.</summary>
	public Item? Item => Read(out _, out _) is { } item ? Definition.Load<Item>(item) : null;

	/// <summary>How many of its item it is; 0 once it is gone.</summary>
	public int Count => Read(out var count, out _) != null ? count : 0;

	/// <summary>Where it lies, in metres.</summary>
	public Vector3 Position => Read(out _, out var at) != null ? at : Vector3.Zero;

	/// <summary>It, as a stack of its item; empty once it is gone.</summary>
	public ItemStack Stack => Read(out var count, out _) is { } item ? new ItemStack(Definition.Load<Item>(item), count) : ItemStack.Empty;

	internal string? Read(out int count, out Vector3 at, bool take = false)
	{
		(count, at) = (0, Vector3.Zero);
		if (!Native.IsConnected || Id == 0)
		{
			return null;
		}
		Span<byte> buffer = stackalloc byte[128];
		int many;
		Vector3 place;
		int length;
		fixed (byte* written = buffer)
		{
			length = Native.Api->ItemDropRead(Id, take ? 1 : 0, written, buffer.Length, &many, &place);
		}
		if (length < 0 || length > buffer.Length)
		{
			return null;
		}
		(count, at) = (many, place);
		return System.Text.Encoding.UTF8.GetString(buffer[..length]);
	}

	public bool Equals(ItemDrop other) => Id == other.Id;

	public override bool Equals(object? obj) => obj is ItemDrop other && Equals(other);

	public override int GetHashCode() => Id.GetHashCode();

	public static bool operator ==(ItemDrop a, ItemDrop b) => a.Equals(b);

	public static bool operator !=(ItemDrop a, ItemDrop b) => !a.Equals(b);
}
