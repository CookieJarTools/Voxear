using System.Numerics;

namespace Voxear.Tests;

internal static class VectorTests
{
	[Test]
	private static void DirectionsAreUnitys()
	{
		Check.Near(new Vector3(0f, 0f, 1f), Vector3.Forward);
		Check.Near(new Vector3(1f, 0f, 0f), Vector3.Right);
		Check.Near(new Vector3(0f, 1f, 0f), Vector3.Up);
		Check.Near(-Vector3.Forward, Vector3.Back);
	}

	[Test]
	private static void MoveTowardsStopsAtTheTarget()
	{
		Check.Near(new Vector3(2f, 0f, 0f), Vector3.MoveTowards(Vector3.Zero, new Vector3(10f, 0f, 0f), 2f));
		Check.Near(new Vector3(1f, 0f, 0f), Vector3.MoveTowards(Vector3.Zero, new Vector3(1f, 0f, 0f), 5f));
	}

	[Test]
	private static void AnglesAreInDegreesAndTurningRightIsPositive()
	{
		Check.Near(90f, Vector3.Angle(Vector3.Forward, Vector3.Right));
		Check.Near(90f, Vector3.SignedAngle(Vector3.Forward, Vector3.Right, Vector3.Up));
		Check.Near(-90f, Vector3.SignedAngle(Vector3.Forward, Vector3.Left, Vector3.Up));
		Check.Near(90f, Vector2.SignedAngle(Vector2.Right, Vector2.Up));
	}

	[Test]
	private static void ProjectionsAndLengths()
	{
		Check.Near(new Vector3(1f, 0f, 2f), Vector3.ProjectOnPlane(new Vector3(1f, 5f, 2f), Vector3.Up));
		Check.Near(new Vector3(0f, 5f, 0f), Vector3.Project(new Vector3(1f, 5f, 2f), Vector3.Up * 3f));
		Check.Near(2f, Vector3.ClampMagnitude(new Vector3(10f, 0f, 0f), 2f).Magnitude);
		Check.Near(Vector3.Zero, Vector3.Zero.Normalized);
		Check.Near(new Vector3(1f, 0f, 3f), new Vector3(1f, 2f, 3f).WithY(0f));
		Check.Near(new Vector2(-2f, 1f), Vector2.Perpendicular(new Vector2(1f, 2f)));
	}
}
