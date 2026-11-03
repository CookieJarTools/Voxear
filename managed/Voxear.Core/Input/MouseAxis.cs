namespace Voxear;

/// <summary>What a mouse reads beyond its buttons, as bindings take them: how far it moved, its wheel, and where the pointer is.</summary>
public enum MouseAxis
{
	/// <summary>How far it moved this frame.</summary>
	Delta,
	/// <summary>Its wheel's notches this frame.</summary>
	Scroll,
	/// <summary>Where the pointer is, in the game's pixels.</summary>
	Position,
}
