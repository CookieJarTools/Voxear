using System;

namespace Voxear;

/// <summary>One row of a recipe's grid: the item in each of its three places, or none.</summary>
[Serializable]
public sealed class RecipeRow
{
	[HideLabel]
	[HorizontalGroup("Row")]
	public Item? Left;

	[HideLabel]
	[HorizontalGroup("Row")]
	public Item? Middle;

	[HideLabel]
	[HorizontalGroup("Row")]
	public Item? Right;

	internal Item?[] Cells => [Left, Middle, Right];
}
