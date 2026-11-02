using System.Numerics;

namespace Voxear;

/// <summary>Where a ray met a 2D collider, from Physics2D.Raycast.</summary>
public readonly struct RaycastHit2D
{
	internal RaycastHit2D(Obj obj, Vector2 point, Vector2 normal, float distance)
	{
		Obj = obj;
		Point = point;
		Normal = normal;
		Distance = distance;
	}

	/// <summary>The Obj whose collider the ray met.</summary>
	public Obj Obj { get; }

	public Collider2D Collider => Obj.Collider2D;

	/// <summary>The Obj's Rigidbody2D, which is not valid when physics does not move it.</summary>
	public Rigidbody2D Rigidbody => Obj.Rigidbody2D;

	/// <summary>Where the ray met the collider, in the XY plane.</summary>
	public Vector2 Point { get; }

	/// <summary>Which way the edge the ray met faces.</summary>
	public Vector2 Normal { get; }

	/// <summary>How far the ray went before it met the collider, in metres.</summary>
	public float Distance { get; }
}
