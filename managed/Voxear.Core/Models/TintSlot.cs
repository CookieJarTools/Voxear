namespace Voxear;

/// <summary>
/// Which of a Model Renderer's three tints. Without a tint mask the first tints the whole model; a box model whose
/// colours take other tints, or a material with a tint mask, takes each where it says.
/// </summary>
public enum TintSlot
{
	First,
	Second,
	Third,
}
