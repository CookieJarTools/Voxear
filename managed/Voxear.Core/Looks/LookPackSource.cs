namespace Voxear;

/// <summary>Where a look pack is from.</summary>
public enum LookPackSource
{
	/// <summary>One of the engine's own.</summary>
	BuiltIn,

	/// <summary>A <c>.lookpack</c> asset of the game's.</summary>
	Game,

	/// <summary>A file the player put in the game's Looks folder.</summary>
	Player,
}
