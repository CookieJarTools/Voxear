namespace Voxear;

/// <summary>How the room the listener is in rings, with Audio.Reverb on.</summary>
public enum ReverbMode
{
	/// <summary>A reverb that follows how long the room rings: cheap, and smooth in any room.</summary>
	Parametric,

	/// <summary>
	/// The room's own echoes, from Steam Audio, each at its own time and from its own side: closer to the real room,
	/// for about two percent of a core more.
	/// </summary>
	Convolution,
}
