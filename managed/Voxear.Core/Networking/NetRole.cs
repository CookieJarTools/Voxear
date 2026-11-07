namespace Voxear;

/// <summary>What this machine is in networked play.</summary>
public enum NetRole
{
	/// <summary>Not playing over the network.</summary>
	None,

	/// <summary>Runs the game for its players, with no player of its own.</summary>
	Server,

	/// <summary>Runs the game for its players and plays it too.</summary>
	Host,

	/// <summary>Plays a game a server runs.</summary>
	Client,
}
