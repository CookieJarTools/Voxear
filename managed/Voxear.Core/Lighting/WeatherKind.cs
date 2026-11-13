namespace Voxear;

/// <summary>Weather by name: each a set of <see cref="WeatherAmounts"/>.</summary>
public enum WeatherKind
{
	/// <summary>The level's lighting as it is.</summary>
	Clear,

	/// <summary>Half the sky in cloud.</summary>
	Cloudy,

	/// <summary>A grey sky from edge to edge and a dull day under it.</summary>
	Overcast,

	/// <summary>Steady rain from a dark sky, with some wind and mist.</summary>
	Rain,

	/// <summary>Heavy rain, a gale and a sky near black.</summary>
	Storm,

	/// <summary>Snow falling from a pale, close sky.</summary>
	Snow,

	/// <summary>Thick fog, the sky lost in it.</summary>
	Fog,

	/// <summary>Amounts of its own, as <see cref="Weather.Set(WeatherAmounts, float)"/> gives them.</summary>
	Custom,
}
