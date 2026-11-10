using System;

namespace Voxear;

/// <summary>
/// Some of one item, as an inventory's place holds them or a block drops them: the item, how many, and how worn it
/// is where it wears. One with no item or none of it is empty.
/// </summary>
[Serializable]
public struct ItemStack : IEquatable<ItemStack>
{
	public static readonly ItemStack Empty = default;

	public Item? Item;

	[MinValue(0)]
	public int Count;

	[Tooltip("How many blocks it has broken, of its item's Durability")]
	[MinValue(0)]
	public int Wear;

	public ItemStack(Item? item, int count = 1, int wear = 0)
	{
		(Item, Count, Wear) = (item, count, wear);
	}

	public readonly bool IsEmpty => Item == null || Count <= 0;

	/// <summary>How many more of its item it has room for.</summary>
	public readonly int Room => Item == null ? 0 : Math.Max(Item.StackSize - Count, 0);

	/// <summary>Whether <paramref name="other"/> is more of the same, to go on one pile with it: the same item, neither worn.</summary>
	public readonly bool StacksWith(ItemStack other)
	{
		return !IsEmpty && !other.IsEmpty && Item == other.Item && Wear == 0 && other.Wear == 0;
	}

	/// <summary>
	/// The stack after its item broke one more block: more worn, and one fewer of it once it is worn out. An item
	/// with no Durability never wears.
	/// </summary>
	public readonly ItemStack Worn()
	{
		if (Item == null || Item.Durability <= 0)
		{
			return this;
		}
		return Wear + 1 >= Item.Durability ? new ItemStack(Count > 1 ? Item : null, Count - 1) : this with { Wear = Wear + 1 };
	}

	public readonly bool Equals(ItemStack other)
	{
		return (IsEmpty && other.IsEmpty) || (Item == other.Item && Count == other.Count && Wear == other.Wear);
	}

	public override readonly bool Equals(object? other)
	{
		return other is ItemStack stack && Equals(stack);
	}

	public override readonly int GetHashCode()
	{
		return IsEmpty ? 0 : HashCode.Combine(Item, Count, Wear);
	}

	public static bool operator ==(ItemStack left, ItemStack right)
	{
		return left.Equals(right);
	}

	public static bool operator !=(ItemStack left, ItemStack right)
	{
		return !left.Equals(right);
	}

	public override readonly string ToString()
	{
		return IsEmpty ? "nothing" : $"{Count} {Item!.Name}";
	}
}
