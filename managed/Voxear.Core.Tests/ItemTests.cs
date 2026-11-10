using System;
using System.Collections.Generic;

namespace Voxear.Tests;

internal static class ItemTests
{
	private static Item Named(string name, int stack = 64)
	{
		return new Item { Name = name, StackSize = stack };
	}

	private static Recipe Shaped(Item result, params Item?[] cells)
	{
		var recipe = new Recipe { Name = result.Name, Result = result };
		(recipe.Top.Left, recipe.Top.Middle, recipe.Top.Right) = (cells[0], cells[1], cells[2]);
		(recipe.Middle.Left, recipe.Middle.Middle, recipe.Middle.Right) = (cells[3], cells[4], cells[5]);
		(recipe.Bottom.Left, recipe.Bottom.Middle, recipe.Bottom.Right) = (cells[6], cells[7], cells[8]);
		return recipe;
	}

	[Test]
	private static void AShapedRecipeIsMadeAnywhereInTheGridAndMirrored()
	{
		var (plank, stick, axe) = (Named("Plank"), Named("Stick"), Named("Axe", 1));
		// Two planks over a plank and a stick over a stick: an axe, its head to the left.
		var recipe = Shaped(axe, plank, plank, null, plank, stick, null, null, stick, null);
		Item?[] asDrawn = [plank, plank, null, plank, stick, null, null, stick, null];
		Item?[] moved = [null, plank, plank, null, plank, stick, null, null, stick];
		Item?[] mirrored = [plank, plank, null, stick, plank, null, stick, null, null];
		Item?[] another = [plank, plank, null, stick, stick, null, null, stick, null];
		Check.True(recipe.Matches(asDrawn, 3), "as its grid shows it");
		Check.True(recipe.Matches(moved, 3), "moved a place to the right");
		Check.True(recipe.Matches(mirrored, 3), "mirrored left to right");
		Check.True(!recipe.Matches(another, 3), "a stick where a plank goes is another thing");
		Item?[] more = [plank, plank, stick, plank, stick, null, null, stick, null];
		Check.True(!recipe.Matches(more, 3), "an item more than it asks for is not it");
		// Two wide and three high, it has no room in a grid two by two, and fits one four wide.
		Item?[] small = [plank, plank, plank, stick];
		Check.True(!recipe.Matches(small, 2), "too large for a small grid");
		var wide = new Item?[16];
		(wide[5], wide[6], wide[9], wide[10], wide[14]) = (plank, plank, plank, stick, stick);
		Check.True(recipe.Matches(wide, 4), "in the middle of a grid four wide");
		Check.True(!recipe.Matches(new Item?[9], 3), "an empty grid makes nothing");
		Check.True(!Shaped(axe, new Item?[9]).Matches(new Item?[9], 3), "nor does an empty recipe");
	}

	[Test]
	private static void AShapelessRecipeIsMadeFromItsItemsInAnyPlaces()
	{
		var (flint, iron, lighter) = (Named("Flint"), Named("Iron"), Named("Lighter", 1));
		var recipe = Shaped(lighter, flint, iron, null, null, null, null, null, null, null);
		recipe.Shape = RecipeShape.Shapeless;
		Item?[] apart = [null, null, iron, null, null, null, flint, null, null];
		Item?[] twice = [flint, flint, iron, null, null, null, null, null, null];
		Check.True(recipe.Matches(apart, 3), "its two items, wherever they lie");
		Check.True(!recipe.Matches(twice, 3), "one flint, not two");
		Check.True(!recipe.Matches([flint, null, null, null], 2), "both of them");
		Check.True(recipe.Matches([iron, null, null, flint], 2), "in a small grid too");
	}

	[Test]
	private static void TheFirstRecipeAGridMakesIsFoundAndOneAtAStationOnlyThere()
	{
		var (ore, coal, ingot, dust) = (Named("Ore"), Named("Coal"), Named("Ingot"), Named("Dust"));
		var furnace = new Station { Name = "Furnace" };
		var smelt = Shaped(ingot, ore, null, null, coal, null, null, null, null, null);
		smelt.Station = furnace;
		var crush = Shaped(dust, ore, null, null, null, null, null, null, null, null);
		List<Recipe> recipes = [smelt, crush];
		Item?[] oreOverCoal = [ore, null, coal, null];
		Item?[] oreAlone = [null, ore, null, null];
		Check.True(Recipes.Match(recipes, oreOverCoal, 2, furnace) == smelt, "at the furnace");
		Check.True(Recipes.Match(recipes, oreOverCoal, 2, null) == null, "not by hand");
		Check.True(Recipes.Match(recipes, oreAlone, 2, null) == crush, "made anywhere, by hand");
		Check.True(Recipes.Match(recipes, oreAlone, 2, furnace) == crush, "and at a station");
		Check.True(smelt.Makes == new ItemStack(ingot, 1), "one ingot");
		smelt.Result = null;
		Check.True(Recipes.Match(recipes, oreOverCoal, 2, furnace) == null, "a recipe that makes nothing is none");
	}

	[Test]
	private static void AStackPilesWithMoreOfTheSameAndWearsOut()
	{
		var (stone, pick) = (Named("Stone"), Named("Pick", 1));
		pick.Durability = 2;
		var pile = new ItemStack(stone, 60);
		Check.True(pile.Room == 4 && pile.StacksWith(new ItemStack(stone, 3)), "room for four more stone");
		Check.True(!pile.StacksWith(new ItemStack(pick)) && ItemStack.Empty.IsEmpty, "not with a pick");
		Check.True(new ItemStack(stone, 0) == ItemStack.Empty, "none of something is nothing");
		var worn = new ItemStack(pick).Worn();
		Check.True(worn.Wear == 1 && !worn.IsEmpty, "one block broken");
		Check.True(!worn.StacksWith(new ItemStack(pick)), "a worn one piles with nothing");
		Check.True(worn.Worn().IsEmpty, "the second wears it out");
		Check.True(pile.Worn() == pile, "what has no durability never wears");
	}

	[Test]
	private static void ABlockBreaksFasterWithItsKindOfToolAndDropsOnlyToItsTier()
	{
		var (pickaxe, axe) = (new ToolKind { Name = "Pickaxe" }, new ToolKind { Name = "Axe" });
		var cobble = Named("Cobblestone");
		var stone = BlockInfos.Make("test:stone", new BlockInfo("Stone", 3f, pickaxe, 2, [new BlockDrop(cobble)]));
		var dirt = BlockInfos.Make("test:dirt", new BlockInfo("Dirt", 0.5f, drops: [new BlockDrop(cobble, 2, 2)]));
		var bedrock = BlockInfos.Make("test:bedrock", new BlockInfo("Bedrock") { Unbreakable = true });
		var wooden = new Item { Name = "Wooden Pickaxe", Tool = pickaxe, Tier = 1, Speed = 2f };
		var iron = new Item { Name = "Iron Pickaxe", Tool = pickaxe, Tier = 3, Speed = 6f };
		var hatchet = new Item { Name = "Hatchet", Tool = axe, Tier = 3, Speed = 6f };
		// By hand its hardness; with its kind of tool, as many times faster as the tool is; another kind is a hand.
		Check.Near(3f, stone.BreakTime());
		Check.Near(1.5f, stone.BreakTime(wooden));
		Check.Near(0.5f, stone.BreakTime(iron));
		Check.Near(3f, stone.BreakTime(hatchet));
		Check.Near(0.5f, dirt.BreakTime(iron));
		Check.True(float.IsPositiveInfinity(bedrock.BreakTime(iron)), "nothing breaks it");
		Check.True(Block.None.BreakTime() == 0f && !Block.None.IsValid, "no block at all");
		// Its drops go only to its kind of tool of its tier or higher; a block with no tier drops to a hand.
		Check.True(!stone.DropsFor() && !stone.DropsFor(wooden) && !stone.DropsFor(hatchet), "too weak, or the wrong kind");
		Check.True(stone.DropsFor(iron) && dirt.DropsFor() && dirt.DropsFor(hatchet), "the right tool, or none needed");
		Check.True(stone.RollDrops(wooden).Count == 0, "nothing for the wooden one");
		var got = stone.RollDrops(iron, new Random(7));
		Check.True(got.Count == 1 && got[0] == new ItemStack(cobble, 1), "one cobblestone");
		Check.True(dirt.RollDrops()[0].Count == 2, "two from the dirt");
		// A drop with a chance comes that often, and between its fewest and its most.
		var rare = new BlockDrop(cobble, 1, 3, 0.25f);
		var random = new Random(11);
		var (came, most, fewest) = (0, 0, int.MaxValue);
		for (var roll = 0; roll < 4000; roll++)
		{
			var stack = rare.Roll(random);
			if (!stack.IsEmpty)
			{
				came++;
				(most, fewest) = (Math.Max(most, stack.Count), Math.Min(fewest, stack.Count));
			}
		}
		Check.True(came is > 850 and < 1150 && (fewest, most) == (1, 3), $"{came} of 4000, {fewest} to {most}");
		// Which item places a block, and which holds a liquid in another.
		var (bucket, stoneItem) = (Named("Bucket", 16), new Item { Name = "Stone", Places = stone });
		var water = BlockInfos.Make("test:water", new BlockInfo("Water") { Liquid = true });
		var full = new Item { Name = "Water Bucket", Places = water, Leaves = bucket, StackSize = 1 };
		List<Item> items = [bucket, full, stoneItem];
		Check.True(Items.Placing(items, stone) == stoneItem && Items.Placing(items, water) == null, "what places the stone");
		Check.True(Items.Filled(items, bucket, water) == full && Items.Filled(items, stoneItem, water) == null, "the bucket filled");
		BlockInfos.Clear();
	}
}
