namespace Voxear;

/// <summary>What a Prefab Brush does with a click or a stroke in the Scene view.</summary>
public enum BrushMode
{
	/// <summary>Strokes scatter prefabs within its radius, by its density.</summary>
	Paint,

	/// <summary>A click places one, turned by its Turn.</summary>
	Single,
}
