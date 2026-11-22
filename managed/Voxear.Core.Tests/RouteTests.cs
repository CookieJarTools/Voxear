using System.Numerics;

namespace Voxear.Tests;

internal static class RouteTests
{
	[Test]
	private static void ACursorMovesAlongAWayByDistance()
	{
		var way = new RouteWay([Vector3.Zero, new(10f, 0f, 0f), new(10f, 0f, 10f)], 20f, []);
		var (at, heading) = way.At(15f);
		Check.Near(10f, at.X);
		Check.Near(5f, at.Z);
		Check.Near(1f, heading.Z);
		Check.Near(10f, way.At(99f).At.Z);
		Check.Near(0f, way.At(-3f).At.X);
	}
}
