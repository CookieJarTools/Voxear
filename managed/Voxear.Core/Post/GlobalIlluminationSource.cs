namespace Voxear;

/// <summary>Where a level's bounced light comes from.</summary>
public enum GlobalIlluminationSource
{
	/// <summary>The probes round the camera, traced as the game goes: every light's bounce, and nothing to bake.</summary>
	Realtime,

	/// <summary>The level's bake: lightmaps and light probes. The probes round the camera are not traced.</summary>
	Baked,

	/// <summary>The bake for the sky and Mixed and Baked lights, the probes round the camera for Realtime lights.</summary>
	Both,
}
