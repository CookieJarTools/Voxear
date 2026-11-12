namespace Voxear;

/// <summary>What the maps show of the world.</summary>
public enum MapShows
{
	/// <summary>All of it, known or not.</summary>
	Everything = 0,

	/// <summary>What was seen from where the maps follow, as it goes.</summary>
	Explored = 1,

	/// <summary>Only what scripts reveal, with MapKnowledge.</summary>
	GameDecides = 2,
}
