namespace Voxear;

/// <summary>The shape a Dry Area keeps water out of.</summary>
public enum DryShape
{
	/// <summary>A box Size across, turned with its Obj.</summary>
	Box,

	/// <summary>A ball as wide as Size's X.</summary>
	Sphere,

	/// <summary>An upright capsule as wide as Size's X and as tall as its Y.</summary>
	Capsule,

	/// <summary>
	/// The inside of a closed mesh, placed, turned and scaled with its Obj: the one picked in the Inspector, or else
	/// the Obj's Mesh Renderer's.
	/// </summary>
	Mesh,
}
