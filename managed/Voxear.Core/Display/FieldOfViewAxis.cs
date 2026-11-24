namespace Voxear;

/// <summary>Which way a perspective Camera's field of view is measured.</summary>
public enum FieldOfViewAxis
{
	/// <summary>From the bottom of the picture to the top: a wider screen sees more at the sides.</summary>
	Vertical,

	/// <summary>From side to side: a taller screen sees more above and below.</summary>
	Horizontal,
}
