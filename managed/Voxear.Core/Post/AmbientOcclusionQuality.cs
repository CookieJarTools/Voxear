namespace Voxear;

/// <summary>How hard ambient occlusion looks for what shades each pixel: more directions and steps cost more.</summary>
public enum AmbientOcclusionQuality
{
	/// <summary>One direction of four steps.</summary>
	Low,

	/// <summary>Two directions of four steps.</summary>
	Medium,

	/// <summary>Three directions of six steps.</summary>
	High,

	/// <summary>Four directions of eight steps.</summary>
	Ultra,
}
