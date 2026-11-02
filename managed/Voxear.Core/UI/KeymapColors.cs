namespace Voxear;

/// <summary>How a KeymapView colours its keys.</summary>
public enum KeymapColors
{
	/// <summary>Bound, bound only to hidden actions, or unused, as the sheets colour them.</summary>
	Assigned,

	/// <summary>By each key's actions' category, in the colour the input actions give it.</summary>
	Categories,
}
