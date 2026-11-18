namespace Voxear.Tests;

/// <summary>The Nav Agent API with no engine behind it: nothing is sent anywhere, nothing throws.</summary>
internal static class NavAgentTests
{
	[Test]
	private static void ANavAgentSentIntoADimensionWithoutTheEngineGoesNowhere()
	{
		var agent = Obj.None.NavAgent;
		agent.SetDestination(new System.Numerics.Vector3(1f, 2f, 3f), DimensionInstance.Level);
		Check.True(agent.Destination == null && agent.DestinationDimension.IsLevel, "nothing to send");
	}
}
