namespace Voxear;

/// <summary>What a storm is told to be.</summary>
public enum StormKind
{
	/// <summary>
	/// Whatever the temperature makes it at each place: rain, with thunder when it blows hard, sleet round freezing,
	/// snow and blizzards below it, and blown sand where it is hot and dry.
	/// </summary>
	ByTemperature,

	/// <summary>Rain wherever it is, with no thunder.</summary>
	Rain,

	/// <summary>Rain with thunder and lightning.</summary>
	Thunderstorm,

	/// <summary>Driving snow, whatever the temperature.</summary>
	Blizzard,

	/// <summary>A gale full of blown sand.</summary>
	Sandstorm,

	/// <summary>A bank of thick fog, still and quiet.</summary>
	FogBank,
}
