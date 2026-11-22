namespace Voxear;

/// <summary>Which of a world of blocks' painted maps Painting paints.</summary>
public enum PaintMap
{
	/// <summary>Land, grown with a coast of its own; taken away, sunk into the sea.</summary>
	Land,

	/// <summary>Height, under the generator's own peaks and valleys; taken away, lowered.</summary>
	Height,

	/// <summary>A biome of the world's list, chosen before the climate rules.</summary>
	Biome,

	/// <summary>Every map taken back to the generator alone.</summary>
	Erase,
}
