using System;

namespace Voxear;

/// <summary>
/// Takes up the items lying within Radius of its Obj each frame, as a player walking over what a tree broke into
/// does, and tells of each it took: give them to the player's inventory there.
/// <code>
/// GetBehaviour&lt;ItemPickup&gt;().PickedUp += stack => inventory.Add(stack);
/// </code>
/// </summary>
[AddComponentMenu("Items/Item Pickup")]
public sealed class ItemPickup : Behaviour
{
	/// <summary>How near an item lies to be taken up, in metres.</summary>
	[MinValue(0f)]
	public float Radius = 1.5f;

	/// <summary>Raised for each stack taken up.</summary>
	public event Action<ItemStack>? PickedUp;

	/// <summary>Raised for each stack any Item Pickup takes up, with the one that took it.</summary>
	public static event Action<ItemPickup, ItemStack>? AnyPickedUp;

	protected override void Update()
	{
		if (PickedUp == null && AnyPickedUp == null)
		{
			return;
		}
		foreach (var stack in ItemDrops.Collect(Obj.WorldPosition, Radius))
		{
			PickedUp?.Invoke(stack);
			AnyPickedUp?.Invoke(this, stack);
		}
	}
}
