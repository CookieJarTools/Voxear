namespace Voxear;

/// <summary>What a Length counts in.</summary>
public enum LengthUnit
{
	/// <summary>The UI's pixels, which the display's scale makes larger on a dense screen.</summary>
	Pixels,

	/// <summary>A percentage of the box it is in.</summary>
	Percent,

	/// <summary>Worked out by the layout, as it would be unset.</summary>
	Auto,
}
