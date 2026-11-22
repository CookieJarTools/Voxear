namespace Voxear;

/// <summary>
/// A tag of a World Area and the data asset it carries there. In a world's Area Defaults, the biome it holds for, or
/// Biome.None for anywhere.
/// </summary>
public readonly record struct AreaEntry(AreaTag? Tag, Definition? Data, Biome Biome = default);
