namespace Voxear;

/// <summary>
/// Everything about how plants move at once, for Foliage.Motion: as Project Settings ▸ Foliage has it, with its
/// defaults a new project starts with.
/// </summary>
public readonly record struct FoliageMotion()
{
	/// <summary>
	/// How hard the wind blows over plants on a fair day where the level has no Wind Zone, as a Wind Zone's strength:
	/// 0.5 a light breeze, 1 one that sways trees. Rain and storms blow harder on top; 0 is still until the weather
	/// turns.
	/// </summary>
	public float Wind { get; init; } = 0.5f;

	/// <summary>How far plants bend in the wind and when pushed, times each plant's own Wind bend, up to 3.</summary>
	public float Bend { get; init; } = 1f;

	/// <summary>Whether characters, and Objs with a Foliage Interaction, push plants aside as they pass.</summary>
	public bool Interaction { get; init; } = true;

	/// <summary>How hard they push, times each one's own Strength, up to 3.</summary>
	public float Push { get; init; } = 1f;

	/// <summary>How many seconds a pushed plant takes to spring most of the way back.</summary>
	public float Spring { get; init; } = 0.8f;
}
