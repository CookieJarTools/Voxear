using System.Numerics;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// The water in the level: how high its surface is above a point, which way it faces there, and how it flows. The
/// sea's waves are the ones drawn, worked out the same on every machine, so a boat bobs alike for every player. Over a
/// Pool its surface counts, and over a Stream its current. Where there is no water, as in a Dry Area or a level with
/// no Ocean, Pool or Stream, <see cref="HeightAt"/> gives negative infinity and <see cref="IsUnderwater"/> is false.
/// <code>
/// protected override void Update()
/// {
///     // A buoy riding the waves.
///     var at = Transform.Position;
///     Transform.Position = new Vector3(at.X, Water.HeightAt(at), at.Z);
/// }
/// </code>
/// </summary>
public static unsafe class Water
{
	/// <summary>Whether there is water at the middle of the world, as there is wherever the level has a sea.</summary>
	public static bool Exists => TrySample(Vector3.Zero, out _);

	/// <summary>How high the water's surface is above or below <paramref name="position"/>, in the world.</summary>
	public static float HeightAt(Vector3 position)
	{
		return TrySample(position, out var water) ? water.Height : float.NegativeInfinity;
	}

	/// <summary>The way the water's surface faces above <paramref name="position"/>; straight up where there is none.</summary>
	public static Vector3 NormalAt(Vector3 position)
	{
		return TrySample(position, out var water) ? water.Normal : Vector3.UnitY;
	}

	/// <summary>How fast the water flows at <paramref name="position"/>, in metres a second.</summary>
	public static Vector3 FlowAt(Vector3 position)
	{
		return TrySample(position, out var water) ? water.Flow : Vector3.Zero;
	}

	/// <summary>Whether <paramref name="position"/> is under the water's surface.</summary>
	public static bool IsUnderwater(Vector3 position)
	{
		return TrySample(position, out var water) && position.Y < water.Height;
	}

	/// <summary>The water at <paramref name="position"/>, in one go; false where there is none.</summary>
	public static bool TrySample(Vector3 position, out WaterPoint water)
	{
		var values = stackalloc float[7];
		if (Native.Api->WaterSample(&position, values) == 0)
		{
			water = default;
			return false;
		}
		water = new WaterPoint(values[0], new Vector3(values[1], values[2], values[3]), new Vector3(values[4], values[5], values[6]));
		return true;
	}
}
