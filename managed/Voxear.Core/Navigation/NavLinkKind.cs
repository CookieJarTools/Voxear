namespace Voxear;

/// <summary>How what walks a Nav Link crosses it.</summary>
public enum NavLinkKind
{
	/// <summary>Straight along it, as through a gap in a fence.</summary>
	Walk,

	/// <summary>Over it in a leap, as across a gap too wide to walk.</summary>
	Jump,

	/// <summary>Up or down it slowly, as on a ladder or a rope.</summary>
	Climb,

	/// <summary>From one end to the other at once, as through a door that takes you elsewhere.</summary>
	Teleport,
}
