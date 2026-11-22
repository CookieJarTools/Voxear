namespace Voxear;

/// <summary>When a <see cref="CallAttribute"/> method can be called by a tool.</summary>
public enum CallWhen
{
	/// <summary>While editing, against the level being edited, and while the game plays.</summary>
	EditingAndPlaying,

	/// <summary>Only while editing, not while the game plays.</summary>
	Editing,

	/// <summary>Only while the game plays.</summary>
	Playing,
}
