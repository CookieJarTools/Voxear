using Voxear.Interop;

namespace Voxear;

/// <summary>
/// The level's clock, kept by its Climate: the hour of the day, which turns while the game plays, a whole day every
/// DayLength minutes, and may turn the sun with it. Heat follows it: afternoons are warmest, the hours before dawn
/// coldest. Without a Climate in the level it stands at noon, and setting it says so in the Console.
/// <code>
/// // Sleep till morning.
/// Clock.TimeOfDay = 7f;
/// </code>
/// </summary>
public static unsafe class Clock
{
	private const int TimeValue = 0;
	private const int LengthValue = 1;
	private const int SunValue = 2;
	private const int ExistsValue = 3;

	/// <summary>Whether the level has a Climate to keep its time.</summary>
	public static bool Exists => Read(ExistsValue) == 1f;

	/// <summary>The hour, from 0 to 24: 6.5 is half past six in the morning. It wraps round past midnight.</summary>
	public static float TimeOfDay
	{
		get => Exists ? Read(TimeValue) : 12f;
		set => Write(TimeValue, value);
	}

	/// <summary>How many minutes of play a whole day takes; 0 stands the clock still.</summary>
	public static float DayLength
	{
		get => Read(LengthValue);
		set => Write(LengthValue, value);
	}

	/// <summary>Whether the clock turns the level's first Directional Light, up at six and down at six.</summary>
	public static bool TurnsSun
	{
		get => Read(SunValue) == 1f;
		set => Write(SunValue, value ? 1f : 0f);
	}

	/// <summary>Whether it is day: between six in the morning and six in the evening.</summary>
	public static bool IsDay => TimeOfDay is >= 6f and < 18f;

	private static float Read(int what)
	{
		return Native.IsConnected ? Native.Api->ClockValue(what, 0, 0f) : float.NaN;
	}

	private static void Write(int what, float value)
	{
		if (Native.IsConnected)
		{
			Native.Api->ClockValue(what, 1, value);
		}
	}
}
