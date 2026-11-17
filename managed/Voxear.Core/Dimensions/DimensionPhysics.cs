using System.Numerics;

namespace Voxear;

/// <summary>
/// One dimension's physics: rays that meet only that dimension's colliders, whichever the game is seen from. Got
/// from <see cref="DimensionInstance.Physics"/>.
/// </summary>
public readonly struct DimensionPhysics
{
	private readonly DimensionInstance dimension;

	internal DimensionPhysics(DimensionInstance dimension)
	{
		this.dimension = dimension;
	}

	/// <summary>Whether a ray meets a collider within maxDistance metres, as <see cref="Physics.Raycast(Vector3, Vector3, float, int, bool)"/>.</summary>
	public bool Raycast(Vector3 origin, Vector3 direction, float maxDistance = float.PositiveInfinity, int layerMask = -1, bool hitTriggers = false)
	{
		using var scope = dimension.Use();
		return Physics.Raycast(origin, direction, maxDistance, layerMask, hitTriggers);
	}

	/// <summary>The first collider a ray meets, and where, as <see cref="Physics.Raycast(Vector3, Vector3, out RaycastHit, float, int, bool)"/>.</summary>
	public bool Raycast(Vector3 origin, Vector3 direction, out RaycastHit hit, float maxDistance = float.PositiveInfinity, int layerMask = -1, bool hitTriggers = false)
	{
		using var scope = dimension.Use();
		return Physics.Raycast(origin, direction, out hit, maxDistance, layerMask, hitTriggers);
	}

	/// <summary>The first collider between two points, and where, as <see cref="Physics.Linecast"/>.</summary>
	public bool Linecast(Vector3 start, Vector3 end, out RaycastHit hit, int layerMask = -1, bool hitTriggers = false)
	{
		using var scope = dimension.Use();
		return Physics.Linecast(start, end, out hit, layerMask, hitTriggers);
	}

	/// <summary>The Objs whose colliders a sphere overlaps, as <see cref="Physics.OverlapSphere"/>.</summary>
	public Obj[] OverlapSphere(Vector3 center, float radius, int layerMask = -1, bool hitTriggers = false)
	{
		using var scope = dimension.Use();
		return Physics.OverlapSphere(center, radius, layerMask, hitTriggers);
	}
}
