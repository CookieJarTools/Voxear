using System.Numerics;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// How every plant moves: grass, flowers, bushes and leaves sway in the wind and part round what walks through them.
/// It starts as the project has it, in Project Settings ▸ Foliage; a script changes it while the game runs, from the
/// next frame on, and the project's own settings stay as they are. A level's Wind Zone sets its wind in full: which
/// way it blows, its gusts and its turbulence.
/// <code>
/// // Calm in the valley, and nothing tramples the flower beds.
/// Foliage.Wind = 0.2f;
/// Foliage.Interaction = false;
///
/// // Everything at once.
/// Foliage.Motion = new FoliageMotion { Wind = 1.5f, Bend = 1.2f, Spring = 2f };
/// </code>
/// </summary>
public static class Foliage
{
	/// <summary>
	/// How hard the wind blows over plants on a fair day where the level has no Wind Zone: 0.5 a light breeze, 1 one
	/// that sways trees. Rain and storms blow harder on top. NaN without the engine.
	/// </summary>
	public static float Wind
	{
		get { return NativeFoliage.Get(NativeFoliage.Wind); }
		set { NativeFoliage.Set(NativeFoliage.Wind, value); }
	}

	/// <summary>How far plants bend in the wind and when pushed, times each plant's own Wind bend: 0 holds them still.</summary>
	public static float Bend
	{
		get { return NativeFoliage.Get(NativeFoliage.Bend); }
		set { NativeFoliage.Set(NativeFoliage.Bend, value); }
	}

	/// <summary>Whether characters, and Objs with a Foliage Interaction, push plants aside as they pass.</summary>
	public static bool Interaction
	{
		get { return NativeFoliage.Get(NativeFoliage.Interaction) == 1f; }
		set { NativeFoliage.Set(NativeFoliage.Interaction, value ? 1f : 0f); }
	}

	/// <summary>Everything about how plants move, read or set at once; the defaults without the engine.</summary>
	public static FoliageMotion Motion
	{
		get
		{
			var wind = NativeFoliage.Get(NativeFoliage.Wind);
			if (float.IsNaN(wind))
			{
				return new FoliageMotion();
			}
			return new FoliageMotion
			{
				Wind = wind,
				Bend = NativeFoliage.Get(NativeFoliage.Bend),
				Interaction = NativeFoliage.Get(NativeFoliage.Interaction) == 1f,
				Push = NativeFoliage.Get(NativeFoliage.Push),
				Spring = NativeFoliage.Get(NativeFoliage.Spring),
			};
		}
		set
		{
			NativeFoliage.Set(NativeFoliage.Wind, value.Wind);
			NativeFoliage.Set(NativeFoliage.Bend, value.Bend);
			NativeFoliage.Set(NativeFoliage.Interaction, value.Interaction ? 1f : 0f);
			NativeFoliage.Set(NativeFoliage.Push, value.Push);
			NativeFoliage.Set(NativeFoliage.Spring, value.Spring);
		}
	}

	/// <summary>
	/// The wind plants feel this moment, level: which way it blows along X and Z, as long as it is strong, as a Wind
	/// Zone's strength. From the level's Wind Zone, or the fair day's wind, and the weather where the camera is. Zero
	/// without the engine or in still air.
	/// </summary>
	public static Vector2 Blowing
	{
		get
		{
			var strength = NativeFoliage.Get(NativeFoliage.Blowing);
			if (float.IsNaN(strength))
			{
				return Vector2.Zero;
			}
			return new Vector2(NativeFoliage.Get(NativeFoliage.BlowingX), NativeFoliage.Get(NativeFoliage.BlowingZ)) * strength;
		}
	}
}
