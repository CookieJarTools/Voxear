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

	/// <summary>
	/// Which of the sea's costlier looks are on, as the Ocean's Quality settings in the Inspector: turn some off on
	/// slower machines. <see cref="WaterFeatures.None"/> where the level has no Ocean, and setting them there does
	/// nothing. A change shows from the next frame.
	/// <code>
	/// // On a low quality setting: no foam standing up off the water, and no light shafts under it.
	/// Water.Features &amp;= ~(WaterFeatures.Foam3D | WaterFeatures.LightShafts);
	/// </code>
	/// </summary>
	public static WaterFeatures Features
	{
		get
		{
			var features = Native.Api->WaterFeatures();
			return features < 0 ? WaterFeatures.None : (WaterFeatures)features;
		}
		set => Native.Api->WaterSetFeatures((int)value);
	}

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

	/// <summary>
	/// How fast the water flows at <paramref name="position"/>, in metres a second: a Stream's current, or on the sea
	/// the water carried round by its waves, up a wave's front and down its back.
	/// </summary>
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

	/// <summary>
	/// Throws up a splash where the water's surface is over or under <paramref name="position"/>, as of something
	/// <paramref name="size"/> metres wide going in: drops of water, or blobs of molten rock from lava. False where
	/// there is no water. A Rigidbody splashes by itself as it falls in or ploughs through; this is for what moves
	/// without one, as a character wading does.
	/// <code>
	/// // A footstep in the shallows.
	/// Water.Splash(foot.Position, 0.3f);
	/// </code>
	/// </summary>
	public static bool Splash(Vector3 position, float size = 1f)
	{
		return Native.Api->WaterSplash(&position, size) != 0;
	}
}
