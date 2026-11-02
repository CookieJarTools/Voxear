using System.Numerics;

namespace Voxear;

/// <summary>
/// What a behaviour's OnCollisionEnter and OnCollisionExit are told: the other Obj, and where they touched. On exit
/// they no longer touch, so Point and Normal are zero.
/// </summary>
public readonly struct Collision
{
	internal Collision(Obj obj, Vector3 point, Vector3 normal)
	{
		Obj = obj;
		Point = point;
		Normal = normal;
	}

	/// <summary>The other Obj.</summary>
	public Obj Obj { get; }

	/// <summary>The other Obj's Collider.</summary>
	public Collider Collider => Obj.Collider;

	/// <summary>The other Obj's Rigidbody, which is not valid when physics does not move it.</summary>
	public Rigidbody Rigidbody => Obj.Rigidbody;

	/// <summary>Where they touched first, in the world.</summary>
	public Vector3 Point { get; }

	/// <summary>Which way the other Obj pushes this one: up, for a ball landing on the ground.</summary>
	public Vector3 Normal { get; }
}
