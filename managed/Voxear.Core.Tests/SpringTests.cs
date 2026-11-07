using System;
using System.Numerics;

namespace Voxear.Tests;

internal static class SpringTests
{
	[Test]
	private static void ASpringMovesTheSameAtAnyFrameRate()
	{
		var feel = new SpringFeel(2f, 0.3f);
		Check.Near(Run(feel, 30), Run(feel, 150), 1e-4f);
		var settled = new Spring(0f, feel);
		Check.Near(1f, settled.Update(1f, 30f), 1e-3f);
	}

	[Test]
	private static void DampingBelowOneOvershootsAndANegativeResponseStartsTheWrongWay()
	{
		var bouncy = new Spring(0f, new SpringFeel(2f, 0.2f));
		var highest = 0f;
		for (var i = 0; i < 120; i++)
		{
			highest = MathF.Max(highest, bouncy.Update(1f, 1f / 60f));
		}
		Check.True(highest > 1.3f, $"overshoots: {highest}");
		var wary = new Spring(0f, new SpringFeel(3f, 0.8f, -1f));
		Check.True(wary.Update(1f, 1f / 60f) < 0f, "anticipation starts the wrong way");
	}

	[Test]
	private static void RotationsAndPointsSpringHome()
	{
		var turn = new RotationSpring(Quaternion.CreateFromAxisAngle(Vector3.UnitY, 1f), SpringFeel.HalfLife(0.05f));
		var point = new Spring3(Vector3.One, SpringFeel.HalfLife(0.05f));
		for (var i = 0; i < 60; i++)
		{
			turn.Update(Quaternion.Identity, 1f / 60f);
			point.Update(Vector3.Zero, 1f / 60f);
		}
		Check.True(MathF.Abs(turn.Value.W) > 0.99999f, $"turned home: {turn.Value}");
		Check.Near(Vector3.Zero, point.Value, 1e-3f);
	}

	private static float Run(SpringFeel feel, int fps)
	{
		var spring = new Spring(0f, feel);
		for (var i = 0; i < fps * 6 / 10; i++)
		{
			spring.Update(1f, 1f / fps);
		}
		return spring.Value;
	}
}
