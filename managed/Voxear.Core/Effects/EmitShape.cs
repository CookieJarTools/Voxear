namespace Voxear;

/// <summary>Where a Particle System's particles start from, and which way they go.</summary>
public enum EmitShape
{
	/// <summary>From its Obj's middle, every way.</summary>
	Point,

	/// <summary>From inside a ball, outwards.</summary>
	Sphere,

	/// <summary>From a disc, up its Obj's Y, spreading out to its Angle.</summary>
	Cone,

	/// <summary>From inside a box, up its Obj's Y.</summary>
	Box,
}
