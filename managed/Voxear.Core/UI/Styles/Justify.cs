namespace Voxear;

/// <summary>Where elements sit along the way their box lays them out, and how the space left over is shared.</summary>
public enum Justify
{
	/// <summary>At the start, the default.</summary>
	Start,

	/// <summary>In the middle.</summary>
	Center,

	/// <summary>At the end.</summary>
	End,

	/// <summary>The first at the start, the last at the end, the space shared between them.</summary>
	SpaceBetween,

	/// <summary>The same space round each.</summary>
	SpaceAround,

	/// <summary>The same space between each, and at the ends.</summary>
	SpaceEvenly,
}
