using System.Numerics;

namespace Voxear;

/// <summary>Where a ray met a collider, from Physics.Raycast.</summary>
public readonly struct RaycastHit
{
	internal RaycastHit(Obj obj, Vector3 point, Vector3 normal, float distance, uint plant = 0)
	{
		Obj = obj;
		Point = point;
		Normal = normal;
		Distance = distance;
		Plant = plant == 0 ? null : new Plant(obj, (int)(plant - 1));
	}

	/// <summary>The Obj whose collider the ray met.</summary>
	public Obj Obj { get; }

	public Collider Collider => Obj.Collider;

	/// <summary>The Obj's Rigidbody, which is not valid when physics does not move it.</summary>
	public Rigidbody Rigidbody => Obj.Rigidbody;

	/// <summary>Where the ray met the collider, in the world.</summary>
	public Vector3 Point { get; }

	/// <summary>Which way the surface the ray met faces.</summary>
	public Vector3 Normal { get; }

	/// <summary>How far the ray went before it met the collider, in metres.</summary>
	public float Distance { get; }

	/// <summary>The plant whose trunk the ray met, when it met one of a Foliage's; null for anything else.</summary>
	public Plant? Plant { get; }
}
