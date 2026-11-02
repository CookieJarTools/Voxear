namespace Voxear;

/// <summary>The shape of a Collider2D, flat in the XY plane.</summary>
public enum ColliderShape2D
{
	/// <summary>A rectangle, its Size across X and Y.</summary>
	Box,

	/// <summary>A circle, its Radius round.</summary>
	Circle,

	/// <summary>A capsule standing up: its Height from end to end, and its Radius round.</summary>
	Capsule,
}
