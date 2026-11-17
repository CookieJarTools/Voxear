namespace Voxear;

/// <summary>What a dimension is for, as its file's Kind says, which sets good values for the rest of its settings.</summary>
public enum DimensionKind
{
	/// <summary>Open land under a sky of its own, with weather, on the level's clock.</summary>
	Overworld,

	/// <summary>Enclosed: no sky, no sun, no weather; one for each place that opens it, as each dungeon is.</summary>
	Dungeon,

	/// <summary>Enclosed, as a dungeon is, but only one.</summary>
	Caves,
}
