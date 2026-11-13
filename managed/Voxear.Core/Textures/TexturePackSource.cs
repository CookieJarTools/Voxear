namespace Voxear;

/// <summary>Where a texture pack is from.</summary>
public enum TexturePackSource
{
	/// <summary>One of the engine's own.</summary>
	BuiltIn,

	/// <summary>A <c>.texturepack</c> asset of the game's.</summary>
	Game,

	/// <summary>A folder the player put in the game's Textures folder.</summary>
	Player,
}
