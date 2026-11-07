namespace Voxear;

/// <summary>Who decides what a player's own Objs do, as `authority` in the project's [network] settings says.</summary>
public enum NetAuthority
{
	/// <summary>The server decides everything; players' machines send what they press. Suits games played against others.</summary>
	Server = 0,

	/// <summary>Each player's machine runs their own Objs, and the server passes on how they are. Suits games played together.</summary>
	Shared = 1,
}
