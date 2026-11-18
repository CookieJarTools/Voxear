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

	[Test]
	private static void AnAgentTypeIsTheDefaultOrOneOfTheProjectsOwnFour()
	{
		Check.True(NavAgentType.Own(2).Slot == 2 && !NavAgentType.Own(2).IsDefault, "its own second");
		Check.True(NavAgentType.Own(0) == NavAgentType.Default && NavAgentType.Own(5) == NavAgentType.Default, "out of range");
		Check.True(Obj.None.NavAgent.AgentType == NavAgentType.Default, "nothing to read");
	}
}
