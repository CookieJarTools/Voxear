using Voxear.Interop;

namespace Voxear;

/// <summary>
/// The level's climate, as its Climate component has it, where no world generator gives one: how warm it is, how much
/// the day swings, and how wet. Its clock is Clock. Without a Climate in the level, setting them does nothing and says
/// so in the Console.
/// <code>
/// // A cold snap.
/// Climate.Celsius -= 12f;
/// Climate.Humidity = 0.2f;
/// </code>
/// </summary>
public static class Climate
{
	/// <summary>Whether the level has a Climate.</summary>
	public static bool Exists => Level.Exists;

	/// <summary>How warm it is at nothing high, in °C, where no generator says.</summary>
	public static float Celsius
	{
		get => Level.Number("celsius", 15f);
		set => Level.Change("celsius", value);
	}

	/// <summary>How much warmer the afternoon is, and colder the small hours, in °C.</summary>
	public static float DaySwing
	{
		get => Level.Number("day_swing", 8f);
		set => Level.Change("day_swing", value);
	}

	/// <summary>How wet it is, from 0 to 1, where no generator says.</summary>
	public static float Humidity
	{
		get => Level.Number("humidity", 0.5f);
		set => Level.Change("humidity", value);
	}

	private static LevelSettings Level { get; } = new("climate", "Climate");
}
