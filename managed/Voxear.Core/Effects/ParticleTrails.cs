namespace Voxear;

/// <summary>What a Particle System's particles leave behind them as they go.</summary>
public enum ParticleTrails
{
	/// <summary>Nothing.</summary>
	Off,

	/// <summary>Each particle draws a tail along the way it has come, narrowing and fading to its end: sparks, embers.</summary>
	Particles,

	/// <summary>
	/// The particles are joined into one ribbon in the order they were sent out, and not drawn themselves: the streak
	/// behind a spell, a blade or a rocket, from a system in the World space on an Obj that moves.
	/// </summary>
	Ribbon,
}
