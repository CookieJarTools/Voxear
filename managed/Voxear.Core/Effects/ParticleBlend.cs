namespace Voxear;

/// <summary>How a Particle System's particles show over what is behind them.</summary>
public enum ParticleBlend
{
	/// <summary>Covering it as much as they are opaque: smoke, dust, spray.</summary>
	Alpha,

	/// <summary>Adding their light to it: sparks, fire, magic.</summary>
	Additive,
}
