namespace Voxear;

/// <summary>What shape a Light shines from.</summary>
public enum LightKind
{
	/// <summary>From far away along the Obj's forward (Z), as the sun does, over the whole level.</summary>
	Directional,

	/// <summary>From a point, every way, out to its Range.</summary>
	Point,

	/// <summary>From a point, in a cone along the Obj's forward, between its inner and outer angles.</summary>
	Spot,

	/// <summary>From a rectangle's face, Width by Height, along the Obj's forward: a window or a panel.</summary>
	Rectangle,

	/// <summary>From a disc's face, Radius round, along the Obj's forward.</summary>
	Disc,

	/// <summary>From a glowing tube, Length along the Obj's X and Radius round it, as a fluorescent tube.</summary>
	Tube,
}
