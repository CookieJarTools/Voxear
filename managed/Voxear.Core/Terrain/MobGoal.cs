namespace Voxear;

/// <summary>What a Block Mob is doing about where it is.</summary>
public enum MobGoal
{
	/// <summary>Standing where it is.</summary>
	Stand,

	/// <summary>Walking to a point.</summary>
	MoveTo,

	/// <summary>Keeping near an Obj.</summary>
	Follow,

	/// <summary>Going here and there near where it began.</summary>
	Wander,
}
