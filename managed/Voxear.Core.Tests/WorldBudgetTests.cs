namespace Voxear.Tests;

/// <summary>What making worlds of blocks may spend, with no engine behind it: it reads as High, and nothing throws.</summary>
internal static class WorldBudgetTests
{
	[Test]
	private static void WithoutTheEngineTheQualityReadsAsHighAndSettingItDoesNothing()
	{
		Check.True(World.Quality == WorldQuality.High, "High until the project says otherwise");
		World.Quality = WorldQuality.Low;
		World.ColumnsAtOnce = 2;
		World.ViewDistance = 96f;
		Check.True(World.Quality == WorldQuality.High && World.ColumnsAtOnce == 0, "nothing set without the engine");
		Check.True(float.IsNaN(World.FarDistance), "a distance reads as NaN");
	}
}
