using System.Numerics;

namespace Voxear.Tests
{
	internal static class QuaternionTests
	{
		[Test]
		private static void YawingRightTurnsForwardToRight()
		{
			Check.Near(Vector3.Right, Quaternion.Euler(0f, 90f, 0f) * Vector3.Forward);
			Check.Near(Vector3.Right, Quaternion.AngleAxis(90f, Vector3.Up) * Vector3.Forward);
			// Pitching down by a positive X angle, as in Unity.
			Check.Near(Vector3.Down, Quaternion.Euler(90f, 0f, 0f) * Vector3.Forward);
		}

		[Test]
		private static void EulerAnglesComeBack()
		{
			Check.Near(new Vector3(30f, 45f, 10f), Quaternion.Euler(30f, 45f, 10f).EulerAngles, 1e-2f);
			Check.Near(new Vector3(330f, 0f, 0f), Quaternion.Euler(-30f, 0f, 0f).EulerAngles, 1e-2f);
			Check.Near(new Vector3(90f, 40f, 0f), Quaternion.Euler(90f, 40f, 0f).EulerAngles, 1e-1f);
		}

		[Test]
		private static void LookRotationFacesForwardWithUpKeptUp()
		{
			var look = Quaternion.LookRotation(new Vector3(1f, 0f, 1f));
			Check.Near(Vector3.Normalize(new Vector3(1f, 0f, 1f)), look.Forward);
			Check.Near(Vector3.Up, look.Up);
			Check.Same(Quaternion.Euler(0f, 45f, 0f), look);

			var straightUp = Quaternion.LookRotation(Vector3.Up);
			Check.Near(Vector3.Up, straightUp.Forward);
			Check.Same(Quaternion.Identity, Quaternion.LookRotation(Vector3.Zero));
		}

		[Test]
		private static void FromToTurnsOneDirectionIntoTheOther()
		{
			Check.Near(Vector3.Right, Quaternion.FromToRotation(Vector3.Forward, Vector3.Right) * Vector3.Forward);
			Check.Near(Vector3.Back, Quaternion.FromToRotation(Vector3.Forward, Vector3.Back) * Vector3.Forward);
			Check.Same(Quaternion.Identity, Quaternion.FromToRotation(Vector3.Up, Vector3.Up * 2f));
		}

		[Test]
		private static void RotateTowardsIsCappedInDegrees()
		{
			var turned = Quaternion.RotateTowards(Quaternion.Identity, Quaternion.Euler(0f, 90f, 0f), 30f);
			Check.Near(30f, Quaternion.Angle(Quaternion.Identity, turned), 1e-2f);
			Check.Same(Quaternion.Euler(0f, 90f, 0f), Quaternion.RotateTowards(Quaternion.Identity, Quaternion.Euler(0f, 90f, 0f), 120f));
		}
	}
}
