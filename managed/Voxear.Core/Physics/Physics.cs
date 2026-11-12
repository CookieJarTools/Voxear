using System.Numerics;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// Questions about the colliders in the world, as Unity's Physics: what a ray meets, and what a sphere overlaps.
/// They see the colliders where the last physics step left them.
/// <para>
/// Unlike Unity's, the questions pass over triggers unless hitTriggers is true, so a ray across a checkpoint finds
/// the wall behind it. As in Unity, a ray that starts inside a collider passes out of it without meeting it, and
/// layerMask keeps a question to the Objs on some layers, every layer when left out.
/// </para>
/// <code>
/// if (Physics.Raycast(Obj.WorldPosition, Vector3.UnitY * -1f, out var hit, 2f, LayerMask.GetMask("Ground")))
/// {
///     Log.Info($"Standing on {hit.Obj.Name}, {hit.Distance:0.00} m down");
/// }
/// </code>
/// </summary>
public static class Physics
{
	/// <summary>Which way and how hard everything falls, in metres per second per second. Down at 9.81 to start with.</summary>
	public static Vector3 Gravity
	{
		get { return NativePhysics.Vector(Obj.None, NativePhysics.Gravity, new Vector3(0f, -9.81f, 0f)); }
		set { NativePhysics.Set(Obj.None, NativePhysics.Gravity, new Vector4(value, 0f)); }
	}

	/// <summary>
	/// Which way and how hard things fall at a point: Gravity, the same everywhere.
	/// <code>
	/// falling += Physics.GravityAt(Obj.WorldPosition) * Time.Delta;
	/// </code>
	/// </summary>
	public static Vector3 GravityAt(Vector3 position)
	{
		return NativePhysics.GravityAt(position, Gravity);
	}

	/// <summary>Whether a ray from origin along direction meets a collider within maxDistance metres.</summary>
	public static bool Raycast(Vector3 origin, Vector3 direction, float maxDistance = float.PositiveInfinity, int layerMask = -1, bool hitTriggers = false)
	{
		return NativePhysics.Raycast(origin, direction, maxDistance, layerMask, hitTriggers, out _);
	}

	/// <summary>The first collider a ray from origin along direction meets within maxDistance metres, and where.</summary>
	public static bool Raycast(Vector3 origin, Vector3 direction, out RaycastHit hit, float maxDistance = float.PositiveInfinity, int layerMask = -1, bool hitTriggers = false)
	{
		if (!NativePhysics.Raycast(origin, direction, maxDistance, layerMask, hitTriggers, out var found))
		{
			hit = default;
			return false;
		}

		var point = new Vector3(found.PointX, found.PointY, found.PointZ);
		var normal = new Vector3(found.NormalX, found.NormalY, found.NormalZ);
		hit = new RaycastHit(new Obj(found.Obj), point, normal, found.Distance, found.Plant);
		return true;
	}

	/// <summary>The first collider between two points, and where, as Unity's Linecast.</summary>
	public static bool Linecast(Vector3 start, Vector3 end, out RaycastHit hit, int layerMask = -1, bool hitTriggers = false)
	{
		var along = end - start;
		return Raycast(start, along, out hit, along.Length(), layerMask, hitTriggers);
	}

	/// <summary>The Objs whose colliders a sphere overlaps, each once: what an explosion reaches, say.</summary>
	public static Obj[] OverlapSphere(Vector3 center, float radius, int layerMask = -1, bool hitTriggers = false)
	{
		return NativePhysics.Overlap(center, radius, layerMask, hitTriggers);
	}

	/// <summary>Whether a sphere overlaps any collider.</summary>
	public static bool CheckSphere(Vector3 center, float radius, int layerMask = -1, bool hitTriggers = false)
	{
		return NativePhysics.Overlap(center, radius, layerMask, hitTriggers).Length > 0;
	}

	/// <summary>Whether Objs on two layers collide, as the project's collision matrix says.</summary>
	public static bool GetIgnoreLayerCollision(int layer1, int layer2)
	{
		return !NativeLayers.Collide(layer1, layer2);
	}

	/// <summary>Keeps Objs on two layers apart, or lets them collide again, until the game stops.</summary>
	public static void IgnoreLayerCollision(int layer1, int layer2, bool ignore = true)
	{
		NativeLayers.SetCollide(layer1, layer2, !ignore);
	}
}
