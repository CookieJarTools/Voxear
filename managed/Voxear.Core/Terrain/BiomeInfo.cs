namespace Voxear;

/// <summary>What a .biome file says, as scripts read it: its name and its colour.</summary>
internal sealed record BiomeInfo(string Name, Color Color);
