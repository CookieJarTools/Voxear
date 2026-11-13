using System.Numerics;

namespace Voxear.Tests;

internal static class BiomeTests
{
	[Test]
	private static void ABiomeIsItsFileAndNoneWhereNoWorldReaches()
	{
		var desert = BiomeInfos.Make("test:desert", new BiomeInfo("Desert", new Color(0.9f, 0.8f, 0.5f)));
		var forest = BiomeInfos.Make("test:forest", new BiomeInfo("Forest", new Color(0.2f, 0.45f, 0.2f)));
		Check.True(desert.IsValid && desert.Name == "Desert" && desert.Color == new Color(0.9f, 0.8f, 0.5f), "its name and colour");
		Check.True(desert != forest && desert == BiomeInfos.Make("test:desert", new BiomeInfo("Desert", desert.Color)), "the same file is the same biome");
		Check.True(desert.ToString() == "Desert" && Biome.None.ToString() == "None", "as text, its name");
		// None is no biome: nothing to its name, and what a place is away from any world of blocks.
		Check.True(!Biome.None.IsValid && Biome.None.Name == string.Empty && Biome.None.Color == Color.Clear, "none");
		Check.True(Biomes.At(new Vector3(10f, 0f, 10f)) == Biome.None, "no world, no biome");
		Check.True(Biome.Load(string.Empty) == Biome.None, "no path, none");
	}
}
