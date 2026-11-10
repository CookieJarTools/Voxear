using System;
using System.Collections.Generic;

namespace Voxear;

/// <summary>
/// A way to make an item from others: a data asset with a grid of three by three places, each an item or none, what
/// they make and how many. Shaped, the items must lie as the grid shows them, anywhere in the crafting grid or
/// mirrored; shapeless, they may lie anywhere. <see cref="Recipes"/> finds the recipe a grid makes.
/// </summary>
/// <remarks>Every recipe ships in a build, named by anything or not, as they are found all at once.</remarks>
[CreateAsset("Items/Recipe")]
[ContentRoot]
public class Recipe : Definition
{
	public RecipeShape Shape;

	[HideLabel]
	[HorizontalGroup("Top")]
	public Item? TopLeft;

	[HideLabel]
	[HorizontalGroup("Top")]
	public Item? Top;

	[HideLabel]
	[HorizontalGroup("Top")]
	public Item? TopRight;

	[HideLabel]
	[HorizontalGroup("Middle")]
	public Item? Left;

	[HideLabel]
	[HorizontalGroup("Middle")]
	public Item? Middle;

	[HideLabel]
	[HorizontalGroup("Middle")]
	public Item? Right;

	[HideLabel]
	[HorizontalGroup("Bottom")]
	public Item? BottomLeft;

	[HideLabel]
	[HorizontalGroup("Bottom")]
	public Item? Bottom;

	[HideLabel]
	[HorizontalGroup("Bottom")]
	public Item? BottomRight;

	[Title("Makes")]
	public Item? Result;

	[MinValue(1)]
	public int Count = 1;

	[Tooltip("Where it is made: none for anywhere")]
	public Station? Station;

	[Tooltip("How long it takes to make, for a station that takes its time")]
	[MinValue(0)]
	[Suffix("s")]
	public float Time;

	/// <summary>The width of a recipe's own grid.</summary>
	public const int Width = 3;

	/// <summary>Its nine places row by row from the top left, each an item or none.</summary>
	public Item?[] Cells => [TopLeft, Top, TopRight, Left, Middle, Right, BottomLeft, Bottom, BottomRight];

	/// <summary>What it makes: so many of its result.</summary>
	public ItemStack Makes => new(Result, Count);

	/// <summary>
	/// Whether the items in <paramref name="grid"/>, a crafting grid <paramref name="width"/> places wide read row by
	/// row from the top left, are what it is made from, and nothing more.
	/// </summary>
	public bool Matches(ReadOnlySpan<Item?> grid, int width)
	{
		var cells = Cells;
		return Shape == RecipeShape.Shapeless ? SameItems(cells, grid) : SameShape(cells, grid, width);
	}

	private static bool SameItems(Item?[] cells, ReadOnlySpan<Item?> grid)
	{
		var wanted = new List<Item>();
		foreach (var cell in cells)
		{
			if (cell != null)
			{
				wanted.Add(cell);
			}
		}
		if (wanted.Count == 0)
		{
			return false;
		}
		foreach (var given in grid)
		{
			if (given != null && !wanted.Remove(given))
			{
				return false;
			}
		}
		return wanted.Count == 0;
	}

	private static bool SameShape(Item?[] cells, ReadOnlySpan<Item?> grid, int width)
	{
		if (width <= 0 || !Bounds(cells, Width, out var wanted) || !Bounds(grid, width, out var given))
		{
			return false;
		}
		if (wanted.Wide != given.Wide || wanted.High != given.High)
		{
			return false;
		}
		return SameCells(cells, wanted, grid, width, given, false) || SameCells(cells, wanted, grid, width, given, true);
	}

	private static bool SameCells(Item?[] cells, GridBounds wanted, ReadOnlySpan<Item?> grid, int width, GridBounds given, bool mirrored)
	{
		for (var row = 0; row < wanted.High; row++)
		{
			for (var column = 0; column < wanted.Wide; column++)
			{
				var across = mirrored ? wanted.Wide - 1 - column : column;
				var cell = cells[(wanted.Top + row) * Width + wanted.Left + across];
				if (cell != grid[(given.Top + row) * width + given.Left + column])
				{
					return false;
				}
			}
		}
		return true;
	}

	/// <summary>The part of a grid its items lie in; false for a grid with none.</summary>
	private static bool Bounds(ReadOnlySpan<Item?> cells, int width, out GridBounds bounds)
	{
		var (left, top, right, bottom) = (int.MaxValue, int.MaxValue, -1, -1);
		for (var at = 0; at < cells.Length; at++)
		{
			if (cells[at] == null)
			{
				continue;
			}
			var (column, row) = (at % width, at / width);
			(left, right) = (Math.Min(left, column), Math.Max(right, column));
			(top, bottom) = (Math.Min(top, row), Math.Max(bottom, row));
		}
		bounds = new GridBounds(left, top, right - left + 1, bottom - top + 1);
		return right >= 0;
	}
}
