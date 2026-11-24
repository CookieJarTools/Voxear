namespace Voxear;

/// <summary>How a Camera draws what it sees.</summary>
public enum CameraProjection
{
	/// <summary>Further off is smaller, as an eye sees: FieldOfView says how much it sees.</summary>
	Perspective,

	/// <summary>Every ray parallel, the same size however far off: Size says how much it sees.</summary>
	Orthographic,
}
