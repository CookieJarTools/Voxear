namespace Voxear;

/// <summary>What a dab of Painting does to a map.</summary>
public enum PaintMode
{
	/// <summary>Adds to it.</summary>
	Add,

	/// <summary>Takes from it: paints sea, lowers, or takes a biome away.</summary>
	TakeAway,

	/// <summary>Evens it out towards what is round it.</summary>
	Smooth,
}
