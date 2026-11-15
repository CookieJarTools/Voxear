namespace Voxear;

/// <summary>The curve bright light is fitted to a screen by.</summary>
public enum Tonemapper
{
	/// <summary>Graceful highlights and natural colour. Voxear's choice.</summary>
	Agx,

	/// <summary>The filmic curve: punchy and contrasty.</summary>
	Aces,

	/// <summary>Keeps material colours true, for product shots.</summary>
	Neutral,

	/// <summary>The plain curve.</summary>
	Reinhard,

	/// <summary>Clamped only, for finding problems.</summary>
	None,
}
