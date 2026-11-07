using Voxear.Scripting;

namespace Voxear.Tests;

/// <summary>Motion Sets' parts with no engine behind them: references read, and an Obj without a matcher says so.</summary>
internal static class MotionTests
{
	[Test]
	private static void ActionsAndMarksAreAssetsNamedByTheirSet()
	{
		Check.True(FieldValues.IsAsset(typeof(MotionAction)) && FieldValues.IsAsset(typeof(MotionMark)), "fields load them");
		var cast = MotionAction.Load("Sets/Hero.motionset#action:Cast");
		Check.True(cast.IsValid && cast.Name == "Cast", "the action's name");
		Check.True(cast.Set == MotionSet.Load("Sets/Hero.motionset"), "of its set");
		var release = MotionMark.Load("Sets/Hero.motionset#mark:Release");
		Check.True(release == MotionMark.Load("Sets/Hero.motionset#mark:Release"), "the same mark");
		Check.True(release != MotionMark.Load("Sets/Other.motionset#mark:Release"), "of another set, another mark");
		Check.True(!MotionMark.Load("Sets/Hero.motionset#action:Cast").IsValid, "an action is no mark");
		Check.True(!MotionAction.Load(string.Empty).IsValid && MotionAction.None == default, "unset is None");
	}

	[Test]
	private static void AnObjWithoutAMotionMatcherPlaysNothing()
	{
		var matcher = Obj.None.MotionMatcher;
		Check.True(!matcher.IsValid && !matcher.ActionPlaying.IsValid, "no action plays");
		Check.True(matcher.State == MotionState.Grounded && !matcher.CurrentState.IsValid, "no state");
		Check.True(!matcher.MotionSet.IsValid && !matcher.RootMotion, "no set");
	}
}
