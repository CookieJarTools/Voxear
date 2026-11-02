namespace Voxear;

/// <summary>The shape of a Collider.</summary>
public enum ColliderShape
{
	/// <summary>A box, its Size across each axis.</summary>
	Box,

	/// <summary>A ball, its Radius round.</summary>
	Sphere,

	/// <summary>A capsule standing up: its Height from end to end, and its Radius round.</summary>
	Capsule,
}
