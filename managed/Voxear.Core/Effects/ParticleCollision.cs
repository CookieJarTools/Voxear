namespace Voxear;

/// <summary>What a Particle System's particles meet on their way.</summary>
public enum ParticleCollision
{
	/// <summary>Nothing: they pass through everything.</summary>
	Off,

	/// <summary>
	/// What the main view sees, by its depth: the cheapest, and right for sparks, rain and debris in front of the
	/// player. What is out of view or hidden behind something stops nothing.
	/// </summary>
	Depth,
}
