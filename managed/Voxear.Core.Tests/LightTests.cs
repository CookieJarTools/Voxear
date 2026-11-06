namespace Voxear.Tests;

/// <summary>The Light API with no engine behind it: no Obj has a Light, and nothing throws.</summary>
internal static class LightTests
{
	[Test]
	private static void AnObjWithoutALightReadsAsTheDefaults()
	{
		var lamp = Obj.None.Light;
		Check.True(!lamp.IsValid && lamp == Light.None, "no Light on None");
		Check.True(lamp.Kind == LightKind.Point && lamp.Color == Color.White && lamp.Temperature == null, "a white point");
		lamp.Intensity = 3f;
		lamp.Temperature = 2700f;
		lamp.OuterSpotAngle = 40f;
		Check.Near(0f, lamp.Intensity);
		Check.True(Obj.None.AddLight(LightKind.Spot) == Light.None, "nothing to add it to");
	}
}
