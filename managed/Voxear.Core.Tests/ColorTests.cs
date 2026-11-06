namespace Voxear.Tests;

internal static class ColorTests
{
	[Test]
	private static void IntensityBrightensPastWhiteAndBack()
	{
		var glow = new Color(1f, 0.5f, 0f).WithIntensity(2f);
		Check.Near(4f, glow.R);
		Check.Near(2f, glow.G);
		Check.Near(2f, glow.Intensity);
		Check.Near(3f, glow.WithIntensity(3f).Intensity);
		Check.Near(0.5f, glow.WithIntensity(0f).G);
		Check.Near(0f, Color.Grey.Intensity);
	}
}
