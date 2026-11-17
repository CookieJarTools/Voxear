namespace Voxear;

/// <summary>How a Nav Agent moves along the way it finds.</summary>
public enum NavMoves
{
	/// <summary>With its Obj's Character Controller, which collides with blocks, Objs and slopes as a player does.</summary>
	CharacterController,

	/// <summary>
	/// Along the nav itself, kept on the places it can walk: no physics, so hundreds of agents cost little. It goes
	/// round others only by avoiding them.
	/// </summary>
	AlongTheNav,

	/// <summary>Not at all: it finds its way and says the velocity it wants, for a script or the motion matcher to move it.</summary>
	NotAtAll,
}
