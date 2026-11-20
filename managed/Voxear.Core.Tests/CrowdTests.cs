namespace Voxear.Tests;

/// <summary>Crowds with no engine behind them: no Obj is a Crowd Member, the counts are none and the distances the defaults.</summary>
internal static class CrowdTests
{
	[Test]
	private static void NoObjIsACrowdMemberWithoutTheEngine()
	{
		var member = Obj.None.AddCrowdMember();
		Check.True(!member.IsValid && member == Obj.None.CrowdMember, "nothing to add it to");
		member.Tier = CrowdTier.Far;
		member.KeepAsRecord = true;
		Check.True(member.Tier == CrowdTier.ByDistance && !member.KeepAsRecord && member.Importance == 0f, "nothing to read");
		Check.True(member.TierNow == CrowdTier.Near, "everything is near with no engine");
		member.Remove();
		Check.True(member.ToString().Contains("Crowd Member"), member.ToString());
	}

	[Test]
	private static void TheCrowdIsEmptyWithoutTheEngine()
	{
		Check.True(Crowds.Counts == new CrowdCounts(0, 0, 0, 0), Crowds.Counts.ToString());
		Check.True(Crowds.Near == 40f && Crowds.Middle == 150f && Crowds.RecordsBeyond == 250f, "the defaults");
		Crowds.Near = 10f;
		Check.True(Crowds.Enabled, "on");
	}
}
