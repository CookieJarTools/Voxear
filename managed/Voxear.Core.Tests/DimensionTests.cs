namespace Voxear.Tests;

/// <summary>Dimensions with no engine behind them: only the level's is made, and asking for another says why it is not.</summary>
internal static class DimensionTests
{
	[Test]
	private static void AnAssetNamesItsDimensionAndRefsLoadIt()
	{
		var caves = Dimension.Load("Dimensions/Caves.dimension");
		Check.True(caves.IsSet && caves.Reference == "Dimensions/Caves.dimension", "named by its path");
		Check.True(caves == Dimension.Load("Dimensions/Caves.dimension") && caves != Dimension.None, "the same path, the same dimension");
		Check.True(new Ref<Dimension>("Dimensions/Caves.dimension").Load() == caves, "a Ref loads it");
		Check.True(Dimension.None.ToString() == "no dimension" && !Dimension.None.IsSet, "None names none");
	}

	[Test]
	private static void WithoutTheEngineOnlyTheLevelsIsMade()
	{
		Check.True(Dimension.Current.IsLevel && Dimension.Current == DimensionInstance.Level, "seen from the level's");
		Check.True(DimensionInstance.Level.IsMade && DimensionInstance.Level.KeepLoaded, "the level's is always made");
		Check.True(Obj.None.Dimension.IsLevel, "an Obj that is not there is in the level's");
		Check.True(Dimensions.Made.Count == 1 && Dimensions.Made[0].IsLevel, "the level's alone");
		Check.True(Dimension.Load("Dimensions/Caves.dimension").Made() == null, "no other made");
		DimensionInstance.Level.LetGo();
		Check.True(DimensionInstance.Level.IsMade, "the level's is never let go");
	}

	[Test]
	private static void MakingOneWithoutTheEngineIsOverAtOnceAndSaysWhy()
	{
		var making = Dimensions.Make(Dimension.Load("Dimensions/Caves.dimension"));
		Check.True(making.IsDone && !making.IsMade && making.Error != null, "over at once, with why");
		Check.True(making.GetResult().IsLevel, "the level's in its place");
		var moving = Obj.None.MoveTo(DimensionInstance.Level, System.Numerics.Vector3.Zero);
		Check.True(moving.IsDone && !moving.GetResult(), "an Obj that is not there goes nowhere");
	}
}
