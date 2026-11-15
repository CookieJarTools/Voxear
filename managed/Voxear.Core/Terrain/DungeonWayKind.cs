namespace Voxear;

/// <summary>What a way of a dungeon is.</summary>
public enum DungeonWayKind
{
	/// <summary>The grand stair from the entrance down to the first floor.</summary>
	Grand,
	/// <summary>A passage between spaces, with a stair where they differ in height.</summary>
	Passage,
	/// <summary>A narrow way back from the end of a floor to its start.</summary>
	Shortcut,
}
