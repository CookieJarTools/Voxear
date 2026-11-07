namespace Voxear;

/// <summary>
/// What a Motion Set's state is for, as its Role says in the set. A Motion Matcher plays the set's first state for
/// the one it is given with State.
/// </summary>
public enum MotionState
{
	/// <summary>Walking and running about on the ground.</summary>
	Grounded,

	Crouched,

	/// <summary>In the air, jumping or falling.</summary>
	Airborne,

	Swimming,

	/// <summary>A state of the set's own, played only when asked for by its name with SetState.</summary>
	Custom,
}
