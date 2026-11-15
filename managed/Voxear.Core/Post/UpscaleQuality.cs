namespace Voxear;

/// <summary>How much smaller than the output the scene is drawn, as the upscalers' own modes name it.</summary>
public enum UpscaleQuality
{
	/// <summary>Full size: anti-aliasing alone.</summary>
	Native,

	/// <summary>Two thirds of each side.</summary>
	Quality,

	/// <summary>About three fifths of each side.</summary>
	Balanced,

	/// <summary>Half of each side.</summary>
	Performance,

	/// <summary>A third of each side.</summary>
	UltraPerformance,
}
