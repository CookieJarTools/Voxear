namespace Voxear.Tests;

/// <summary>How plants move, with no engine behind it: numbers read as NaN, the whole as its defaults, and nothing throws.</summary>
internal static class FoliageTests
{
	[Test]
	private static void WithoutTheEngineFoliageReadsAsItsDefaultsAndSettingItDoesNothing()
	{
		Foliage.Wind = 2f;
		Foliage.Bend = 0f;
		Foliage.Interaction = false;
		Check.True(float.IsNaN(Foliage.Wind) && float.IsNaN(Foliage.Bend), "nothing set without the engine");
		Check.True(!Foliage.Interaction, "no interaction to read");
		Foliage.Motion = new FoliageMotion { Wind = 1.5f, Spring = 2f };
		var motion = Foliage.Motion;
		Check.True(motion == new FoliageMotion(), "the defaults without the engine");
		Check.Near(0.5f, motion.Wind);
		Check.Near(0.8f, motion.Spring);
		Check.True(motion.Interaction && Foliage.Blowing == System.Numerics.Vector2.Zero, "still air");
	}
}
