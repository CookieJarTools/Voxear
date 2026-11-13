using System.Numerics;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// How hot or cold it is anywhere in the level, in °C: its climate, warmer in the afternoon and colder before dawn,
/// milder in caves and in water, and warmed or cooled by Heat Zones, Heat Sources, hot and cold blocks and lava.
/// <code>
/// // A torch's flame warms the hands of whoever holds it near.
/// if (Heat.At(player.Transform.Position) &lt; 0f)
/// {
///     Draw.ScreenText("You are freezing", Color.Cyan, 0f);
/// }
///
/// // A fireball's blast warms what is round it for three seconds.
/// Heat.AddSource(hit.Point, 600f, 4f, 3f);
/// </code>
/// Food and characters take the temperature where they are through a Heat Body.
/// </summary>
public static unsafe class Heat
{
	private const int Numbers = 8;

	/// <summary>How hot or cold it is at <paramref name="position"/>, in °C.</summary>
	public static float At(Vector3 position)
	{
		return Native.IsConnected ? Native.Api->HeatSample(&position, null) : float.NaN;
	}

	/// <summary>How hot or cold it is at <paramref name="position"/>, and what makes it so.</summary>
	public static HeatSample Sample(Vector3 position)
	{
		if (!Native.IsConnected)
		{
			return default;
		}
		var numbers = stackalloc float[Numbers];
		Native.Api->HeatSample(&position, numbers);
		return new HeatSample(numbers[0], numbers[1], numbers[2], numbers[3], numbers[4], numbers[5], numbers[6], numbers[7]);
	}

	/// <summary>
	/// Sets off heat at <paramref name="position"/> for <paramref name="seconds"/> of the game's time: <paramref
	/// name="celsius"/> there, fading to the place's own by <paramref name="radius"/> metres away. Cold works the same,
	/// below the place's temperature. Returns false when it could not be set off.
	/// </summary>
	public static bool AddSource(Vector3 position, float celsius, float radius, float seconds)
	{
		return Native.IsConnected && Native.Api->HeatAdd(&position, celsius, radius, seconds) == 1;
	}
}
