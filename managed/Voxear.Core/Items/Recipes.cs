using System;
using System.Collections.Generic;

namespace Voxear;

/// <summary>
/// The project's recipes, and which one a grid of items makes.
/// <code>
/// // What the items in a two by two grid make, by hand.
/// var recipe = Recipes.Match(grid, 2);
/// if (recipe != null) result = recipe.Makes;
/// </code>
/// </summary>
public static class Recipes
{
	private static IReadOnlyList<Recipe>? all;

	/// <summary>Every recipe of the project, by path. Read once, and again after <see cref="Refresh"/>.</summary>
	public static IReadOnlyList<Recipe> All => all ??= Definition.All<Recipe>();

	/// <summary>Reads the project's recipes again, as after one was made while the game runs.</summary>
	public static void Refresh()
	{
		all = null;
	}

	/// <summary>
	/// The recipe the items in <paramref name="grid"/> make: a crafting grid <paramref name="width"/> places wide,
	/// read row by row from the top left, each place an item or none. At <paramref name="station"/>, the recipes made
	/// there and those made anywhere; with none, only those made anywhere. Null when they make nothing.
	/// </summary>
	public static Recipe? Match(ReadOnlySpan<Item?> grid, int width, Station? station = null)
	{
		return Match(All, grid, width, station);
	}

	/// <summary>The recipe a grid of stacks makes, each stack's item one of its items.</summary>
	public static Recipe? Match(ReadOnlySpan<ItemStack> grid, int width, Station? station = null)
	{
		var items = new Item?[grid.Length];
		for (var at = 0; at < grid.Length; at++)
		{
			items[at] = grid[at].IsEmpty ? null : grid[at].Item;
		}
		return Match(All, items, width, station);
	}

	/// <summary>Every recipe that makes <paramref name="result"/>, for a book of recipes.</summary>
	public static IEnumerable<Recipe> For(Item result)
	{
		foreach (var recipe in All)
		{
			if (recipe.Result == result)
			{
				yield return recipe;
			}
		}
	}

	/// <summary>The first of <paramref name="recipes"/> a grid makes at a station.</summary>
	internal static Recipe? Match(IReadOnlyList<Recipe> recipes, ReadOnlySpan<Item?> grid, int width, Station? station)
	{
		foreach (var recipe in recipes)
		{
			var here = recipe.Station == null || recipe.Station == station;
			if (here && recipe.Result != null && recipe.Matches(grid, width))
			{
				return recipe;
			}
		}
		return null;
	}
}
