namespace Voxear;

/// <summary>How a Block World's sea is drawn where the weather makes it rough.</summary>
public enum SeaLook
{
	/// <summary>Flat, whatever the weather.</summary>
	Flat,

	/// <summary>Columns a block wide that stand half a block higher or lower.</summary>
	BlockSteps,

	/// <summary>Small cubes, four to a block's side.</summary>
	SmallCubes,

	/// <summary>A smooth swell, as a sea that is not of blocks.</summary>
	Smooth,
}
