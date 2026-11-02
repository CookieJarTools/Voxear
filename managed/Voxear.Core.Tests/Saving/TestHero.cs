using System.Collections.Generic;

namespace Voxear.Tests;

/// <summary>A type saved three ways: version 1 called Health "hp", and version 2 kept Items as one string.</summary>
[SaveVersion(3)]
internal sealed class TestHero
{
	public int Health = 100;
	public List<string> Items = new List<string>();
	public TestBag Bag;

	[MigrateFrom(1)]
	private static void RenameHp(SaveNode hero)
	{
		hero.Rename("hp", "Health");
	}

	[MigrateFrom(2)]
	private static void SplitItems(SaveNode hero)
	{
		hero["Items"] = SaveNode.From((hero["Items"].AsString() ?? string.Empty).Split(','));
	}
}
