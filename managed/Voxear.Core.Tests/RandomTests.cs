using System;

namespace Voxear.Tests
{
	internal static class RandomTests
	{
		[Test]
		private static void RangesStayInBounds()
		{
			for (var i = 0; i < 1000; i++)
			{
				var value = Random.Range(2f, 3f);
				Check.True(value >= 2f && value < 3f, $"{value} is outside [2, 3)");
				Check.True(Random.InsideUnitSphere.Length() <= 1f + 1e-5f, "a point left the sphere");
				Check.Near(1f, Random.OnUnitSphere.Length());
				Check.Near(1f, Random.Rotation.Length());
			}
			Check.True(!Random.Chance(0f) && Random.Chance(1f), "Chance(0) and Chance(1) must be certain");
		}

		[Test]
		private static void SeededGeneratorsRepeat()
		{
			var a = new Random(42);
			var b = new Random(42);
			Check.Near(a.NextFloat(0f, 100f), b.NextFloat(0f, 100f), 0f);
			Check.Near(a.NextOnSphere(), b.NextOnSphere(), 0f);
		}
	}
}
