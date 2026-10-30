namespace Voxear.Tests
{
	internal static class MathfTests
	{
		[Test]
		private static void ClampingAndRemapping()
		{
			Check.Near(1f, Mathf.Clamp(3f, 0f, 1f));
			Check.Near(0.25f, Mathf.InverseLerp(10f, 20f, 12.5f));
			Check.Near(150f, Mathf.Remap(0.5f, 0f, 1f, 100f, 200f));
			Check.Near(15f, Mathf.Lerp(10f, 20f, 0.5f));
			Check.Near(20f, Mathf.Lerp(10f, 20f, 3f), 0f);
		}

		[Test]
		private static void WrappingNeverGoesNegative()
		{
			Check.Near(2f, Mathf.Repeat(-1f, 3f));
			Check.Near(1f, Mathf.PingPong(3f, 2f));
			Check.Near(20f, Mathf.DeltaAngle(350f, 10f));
			Check.Near(-20f, Mathf.DeltaAngle(10f, 350f));
			Check.Near(355f, Mathf.MoveTowardsAngle(350f, 10f, 5f));
		}

		[Test]
		private static void SmoothDampArrivesWithoutOvershooting()
		{
			var value = 0f;
			var velocity = 0f;
			var highest = 0f;
			for (var frame = 0; frame < 120; frame++)
			{
				value = Mathf.SmoothDamp(value, 10f, ref velocity, 0.3f, float.PositiveInfinity, 1f / 60f);
				highest = System.MathF.Max(highest, value);
			}
			Check.Near(10f, value, 0.01f);
			Check.True(highest <= 10f, $"overshot to {highest}");
		}

		[Test]
		private static void DampIsTheSameAtAnyFrameRate()
		{
			var once = Mathf.Damp(0f, 10f, 0.5f, 0.1f);
			var inSteps = 0f;
			for (var step = 0; step < 10; step++)
			{
				inSteps = Mathf.Damp(inSteps, 10f, 0.5f, 0.01f);
			}
			Check.Near(once, inSteps);
		}
	}
}
