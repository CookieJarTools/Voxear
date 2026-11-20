namespace Voxear;

/// <summary>How a Ground Fit meets the ground.</summary>
public enum FitMode
{
	/// <summary>The ground left as it is; only plants cleared, where it clears them.</summary>
	Leave,

	/// <summary>The ground built up under its footprint to its height; higher ground left.</summary>
	Foundation,

	/// <summary>Built up, and the hillside cleared from over it: level ground at its height.</summary>
	Flatten,

	/// <summary>Only cleared down to its height: a cutting.</summary>
	Cut,
}
