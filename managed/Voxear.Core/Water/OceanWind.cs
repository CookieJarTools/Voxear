namespace Voxear;

/// <summary>Where an Ocean's wind comes from.</summary>
public enum OceanWind
{
	/// <summary>Its own: along its Obj's +Z, as its weather says.</summary>
	Own,

	/// <summary>The level's Wind Zone, so one wind sways the plants and raises the sea.</summary>
	WindZone,
}
