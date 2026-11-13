namespace Voxear;

/// <summary>
/// The weather at a place, as <see cref="Storms.At"/> gives it: what it adds there, what that is called, the storm
/// that blows hardest there and how hard, and how warm and wet the open air is.
/// </summary>
public readonly struct WeatherAt
{
	internal WeatherAt(WeatherAmounts amounts, StormWeather kind, float strength, Storm storm, float celsius, float humidity)
	{
		Amounts = amounts;
		Kind = kind;
		Strength = strength;
		Storm = storm;
		Celsius = celsius;
		Humidity = humidity;
	}

	/// <summary>What the weather adds there: the level's own, and every storm's that reaches it.</summary>
	public WeatherAmounts Amounts { get; }

	/// <summary>What that weather is called.</summary>
	public StormWeather Kind { get; }

	/// <summary>How hard the hardest storm blows there, from 0 for none to 1.</summary>
	public float Strength { get; }

	/// <summary>The storm that blows hardest there; <see cref="Storm.None"/> out of every storm.</summary>
	public Storm Storm { get; }

	/// <summary>How warm the open air is there, in °C, which makes rain of a storm or snow.</summary>
	public float Celsius { get; }

	/// <summary>How wet the climate is there, from 0 to 1.</summary>
	public float Humidity { get; }

	/// <summary>Whether a storm blows there.</summary>
	public bool InStorm => Strength > 0f;
}
