using System.Numerics;

namespace Voxear;

/// <summary>
/// What a behaviour's OnCollisionEnter2D and OnCollisionExit2D are told: the other Obj, and where they touched. On
/// exit they no longer touch, so Point and Normal are zero.
/// </summary>
public readonly struct Collision2D
{
	internal Collision2D(Obj obj, Vector2 point, Vector2 normal)
	{
		Obj = obj;
		Point = point;
		Normal = normal;
	}

	/// <summary>The other Obj.</summary>
	public Obj Obj { get; }

	/// <summary>The other Obj's Collider2D.</summary>
	public Collider2D Collider => Obj.Collider2D;

	/// <summary>The other Obj's Rigidbody2D, which is not valid when physics does not move it.</summary>
	public Rigidbody2D Rigidbody => Obj.Rigidbody2D;

	/// <summary>Where they touched first, in the XY plane.</summary>
	public Vector2 Point { get; }

	/// <summary>Which way the other Obj pushes this one: up, for a player landing on a platform.</summary>
	public Vector2 Normal { get; }
}
