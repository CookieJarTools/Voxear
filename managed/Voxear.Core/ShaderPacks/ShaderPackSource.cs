namespace Voxear;

/// <summary>Where a shader pack is from.</summary>
public enum ShaderPackSource
{
	/// <summary>One of the engine's own.</summary>
	BuiltIn,

	/// <summary>A <c>.shaderpack</c> folder of the game's.</summary>
	Game,

	/// <summary>A folder the player put in the game's ShaderPacks folder.</summary>
	Player,
}
