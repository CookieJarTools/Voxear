namespace Voxear;

/// <summary>How the sun's shadows are drawn.</summary>
public enum SunShadows
{
	/// <summary>
	/// Only the parts of a very large shadow map the view needs, as fine as each pixel needs them, kept from frame to
	/// frame: sharp by the camera and out to kilometres. Whatever moves is drawn again each frame.
	/// </summary>
	Virtual,

	/// <summary>Four shadow maps over the shadow distance, drawn whole every frame: the same cost however much moves.</summary>
	Cascades,
}
