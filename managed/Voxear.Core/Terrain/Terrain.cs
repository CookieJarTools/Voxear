using System.Numerics;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// The level's ground: how high it is under a point, which way it faces there, and where a ray meets it, read from
/// every Terrain's heights. It answers with or without physics, the same on every machine. Where no terrain reaches,
/// <see cref="HeightAt"/> gives negative infinity.
/// <code>
/// protected override void Update()
/// {
///     // Keep a marker on the ground as it moves about.
///     var at = Transform.Position;
///     Transform.Position = new Vector3(at.X, Terrain.HeightAt(at), at.Z);
/// }
/// </code>
/// </summary>
public static unsafe class Terrain
{
	/// <summary>Whether the level has a terrain.</summary>
	public static bool Exists => Native.Api->TerrainCount() > 0;

	/// <summary>How high the ground is under <paramref name="position"/>, or negative infinity off every terrain.</summary>
	public static float HeightAt(Vector3 position)
	{
		return TryGetGround(position, out var ground) ? ground.Height : float.NegativeInfinity;
	}

	/// <summary>The way the ground faces under <paramref name="position"/>, smoothed as it is drawn; straight up off it.</summary>
	public static Vector3 NormalAt(Vector3 position)
	{
		return TryGetGround(position, out var ground) ? ground.Normal : Vector3.UnitY;
	}

	/// <summary>The ground under <paramref name="position"/> in one go: its height, its normal and its terrain's Obj.</summary>
	public static bool TryGetGround(Vector3 position, out TerrainGround ground)
	{
		var values = stackalloc float[4];
		ulong obj;
		if (Native.Api->TerrainGround(&position, values, &obj) == 0)
		{
			ground = default;
			return false;
		}
		ground = new TerrainGround(values[0], new Vector3(values[1], values[2], values[3]), new Obj(obj));
		return true;
	}

	/// <summary>
	/// The first place a ray from <paramref name="origin"/> along <paramref name="direction"/> meets the ground
	/// within <paramref name="maxDistance"/> metres, from above or below. Only terrains count: for anything that
	/// collides, terrains among it, use <see cref="Physics.Raycast(Vector3, Vector3, out RaycastHit, float, int, bool)"/>.
	/// </summary>
	public static bool Raycast(Vector3 origin, Vector3 direction, out RaycastHit hit, float maxDistance = float.PositiveInfinity)
	{
		RaycastHitData found;
		if (Native.Api->TerrainRaycast(&origin, &direction, maxDistance, &found) == 0)
		{
			hit = default;
			return false;
		}
		var point = new Vector3(found.PointX, found.PointY, found.PointZ);
		var normal = new Vector3(found.NormalX, found.NormalY, found.NormalZ);
		hit = new RaycastHit(new Obj(found.Obj), point, normal, found.Distance);
		return true;
	}
}
