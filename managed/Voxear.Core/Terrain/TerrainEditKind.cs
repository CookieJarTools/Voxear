namespace Voxear;

/// <summary>What a <see cref="TerrainEdit"/> does to the ground.</summary>
public enum TerrainEditKind
{
	/// <summary>Raises the ground by Amount metres at the middle, easing to nothing at the edge; less than nothing lowers it.</summary>
	Raise = 0,

	/// <summary>A bowl Amount metres deep with a rim thrown up round it, its grass and plants gone, and Surface laid on its walls.</summary>
	Crater = 1,

	/// <summary>Brings the ground to Amount metres high in the world, easing in at the edge.</summary>
	Flatten = 2,

	/// <summary>Lays Surface over the ground, as much as Amount, 0 to 1, at the middle.</summary>
	Paint = 3,

	/// <summary>Cuts the ground away where Cut is set, or fills the holes there again.</summary>
	Holes = 4,

	/// <summary>Brings every kind of grass towards Amount: 0 clears it, 1 grows it fully.</summary>
	Grass = 5,
}
