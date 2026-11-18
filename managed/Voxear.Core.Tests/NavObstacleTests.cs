namespace Voxear.Tests;

/// <summary>The Nav Obstacle API with no engine behind it: no Obj has one, every value reads as none, nothing throws.</summary>
internal static class NavObstacleTests
{
	[Test]
	private static void NoObjHasANavObstacleWithoutTheEngine()
	{
		var obstacle = Obj.None.AddNavObstacle();
		Check.True(!obstacle.IsValid && obstacle == Obj.None.NavObstacle, "nothing to add it to");
		obstacle.Size = new System.Numerics.Vector3(2f, 1f, 2f);
		obstacle.Carve = false;
		Check.True(obstacle.Size == System.Numerics.Vector3.Zero && !obstacle.Carve && !obstacle.IsCarving, "nothing to read");
		Check.True(obstacle.MoveThreshold == 0f && obstacle.StillTime == 0f, "nothing to read");
		obstacle.Remove();
		Check.True(obstacle.ToString().Contains("Nav Obstacle"), obstacle.ToString());
	}
}
