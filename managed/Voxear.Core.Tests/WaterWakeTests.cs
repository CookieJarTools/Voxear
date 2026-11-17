namespace Voxear.Tests;

/// <summary>The Water Wake API with no engine behind it: no Obj has one, every value reads as none, nothing throws.</summary>
internal static class WaterWakeTests
{
	[Test]
	private static void NoObjHasAWaterWakeWithoutTheEngine()
	{
		var wake = Obj.None.AddWaterWake();
		Check.True(!wake.IsValid && wake == Obj.None.WaterWake, "nothing to add it to");
		wake.Size = 2f;
		wake.Enabled = false;
		Check.True(wake.Size == 0f && wake.Strength == 0f && wake.Foam == 0f && !wake.Enabled, "nothing to read");
		wake.Remove();
		Check.True(wake.ToString().Contains("Water Wake"), wake.ToString());
	}
}
