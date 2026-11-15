namespace Voxear;

/// <summary>What a space of a dungeon is.</summary>
public enum DungeonSpaceKind
{
	/// <summary>The foot of the grand stair down from the entrance.</summary>
	Landing,
	/// <summary>A room where fights happen.</summary>
	Room,
	/// <summary>A pillared hall with statues in alcoves and a gallery.</summary>
	Hall,
	/// <summary>A deep cut in the rock with a stair down its wall.</summary>
	Gorge,
	/// <summary>A pit glowing red, walkways round it and bridges over it.</summary>
	Chasm,
	/// <summary>A hall flooded round carved monoliths.</summary>
	Flooded,
	/// <summary>The boss's arena.</summary>
	Arena,
	/// <summary>A round shaft down to the next floor.</summary>
	Descent,
	/// <summary>A small room off the way where the loot is.</summary>
	Treasury,
	/// <summary>A small room to rest in.</summary>
	Shrine,
}
