namespace Voxear;

/// <summary>How lamps' shadows are drawn.</summary>
public enum LampShadows
{
	/// <summary>Only the parts of each lamp's shadow map the view needs, up to eight times as sharp as the atlas.</summary>
	Virtual,

	/// <summary>A fixed size of view for each lamp.</summary>
	Atlas,
}
