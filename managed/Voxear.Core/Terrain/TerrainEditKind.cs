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

	/// <summary>Digs a ball of ground Radius metres round out of it, up and down too, laying Surface on what it bares.</summary>
	Dig = 6,

	/// <summary>Puts a ball of ground Radius metres round back.</summary>
	Fill = 7,

	/// <summary>Digs a tunnel Radius metres round from Point to To: a trench along the ground, or a way under it.</summary>
	Tunnel = 8,

	/// <summary>Blows a crater out of the ground at Point, Radius metres round, its lip thrown up, laying Surface on what it bares.</summary>
	Explode = 9,
}
