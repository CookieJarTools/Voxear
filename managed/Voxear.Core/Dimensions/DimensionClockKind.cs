namespace Voxear;

/// <summary>Whose time of day a dimension keeps.</summary>
public enum DimensionClockKind
{
	/// <summary>The level's: its sun stands where the level's does.</summary>
	Level,

	/// <summary>Its own first Climate's.</summary>
	Own,

	/// <summary>None: its sun, if it has one, stays where it was put.</summary>
	None,
}
