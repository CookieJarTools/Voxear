namespace Voxear;

/// <summary>What the weather is at a place, by name, as <see cref="Storms.At"/> tells it.</summary>
public enum StormWeather
{
	/// <summary>Nothing falls and no fog lies.</summary>
	None,

	Rain,

	/// <summary>Rain with lightning.</summary>
	Thunderstorm,

	/// <summary>Rain and snow together, round freezing.</summary>
	Sleet,

	Snow,

	/// <summary>Heavy snow in a gale.</summary>
	Blizzard,

	/// <summary>Blown sand.</summary>
	Sandstorm,

	Fog,
}
