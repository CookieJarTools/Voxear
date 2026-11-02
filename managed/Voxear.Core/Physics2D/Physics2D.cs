using System.Numerics;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// Questions about the 2D colliders in the XY plane, as Unity's Physics2D: what a ray meets, and what a circle
/// overlaps. They see the colliders where the last physics step left them, and pass over triggers unless hitTriggers
/// is true.
/// <code>
/// var grounded = Physics2D.Raycast(feet, -Vector2.UnitY, out _, 0.1f);
/// </code>
/// </summary>
public static class Physics2D
{
	/// <summary>Which way and how hard everything in 2D falls, in metres per second per second. Down at 9.81 to start with.</summary>
	public static Vector2 Gravity
	{
		get { return NativePhysics.Vector2(Obj.None, NativePhysics.Gravity2D, new Vector2(0f, -9.81f)); }
		set { NativePhysics.Set(Obj.None, NativePhysics.Gravity2D, new Vector4(value, 0f, 0f)); }
	}

	/// <summary>The first 2D collider a ray from origin along direction meets within maxDistance metres, and where.</summary>
	public static bool Raycast(Vector2 origin, Vector2 direction, out RaycastHit2D hit, float maxDistance = float.PositiveInfinity, bool hitTriggers = false)
	{
		if (!NativePhysics.Raycast2D(origin, direction, maxDistance, hitTriggers, out var found))
		{
			hit = default;
			return false;
		}

		var point = new Vector2(found.PointX, found.PointY);
		var normal = new Vector2(found.NormalX, found.NormalY);
		hit = new RaycastHit2D(new Obj(found.Obj), point, normal, found.Distance);
		return true;
	}

	/// <summary>The first 2D collider between two points, and where.</summary>
	public static bool Linecast(Vector2 start, Vector2 end, out RaycastHit2D hit, bool hitTriggers = false)
	{
		var along = end - start;
		return Raycast(start, along, out hit, along.Length(), hitTriggers);
	}

	/// <summary>The Objs whose 2D colliders a circle overlaps, each once.</summary>
	public static Obj[] OverlapCircle(Vector2 center, float radius, bool hitTriggers = false)
	{
		return NativePhysics.Overlap2D(center, radius, hitTriggers);
	}
}
