using System.Linq;

namespace Voxear.Tests;

internal static class ContentRootTests
{
	[Test]
	private static void RootsAreFoundOnTypesAndMembersWithWhereTheyCameFrom()
	{
		var roots = ContentRoots.Find(typeof(ContentRootTests).Assembly);

		Check.True(roots.Contains(("Prefabs/Bosses/**", "Voxear.Tests.BossSpawner")), "the class's first pattern");
		Check.True(roots.Contains(("Levels/Arena.level", "Voxear.Tests.BossSpawner")), "and its second");
		Check.True(roots.Contains(("Prefabs/Minion.prefab", "Voxear.Tests.BossSpawner.minions")), "a field's");
		Check.True(roots.Contains(("Materials/Rage.material", "Voxear.Tests.BossSpawner.Enrage")), "a method's");
		Check.True(roots.Count == 4, $"nothing else, got {roots.Count}");
	}

	[Test]
	private static void TheEngineGetsOneLinePerPattern()
	{
		var lines = ContentRoots.Describe(new[] { typeof(ContentRootTests).Assembly }).Split('\n');

		Check.True(lines.Contains("Prefabs/Minion.prefab\tVoxear.Tests.BossSpawner.minions"), "pattern, tab, source");
		// A Definition whose every data asset ships has a line of its own: the engine's recipes, and a game's kind of them.
		Check.True(lines.Contains("type:Voxear.Recipe\tVoxear.Recipe"), "every recipe");
		Check.True(lines.Contains("type:Voxear.Tests.TestSmelting\tVoxear.Tests.TestSmelting"), "and a recipe of the game's own");
		Check.True(!lines.Any(line => line.StartsWith("type:Voxear.Item")), "an item ships when something names it");
		Check.True(lines.Length == 7 && lines[^1].Length == 0, $"six lines, each ending in a newline, got {lines.Length - 1}");
	}
}
