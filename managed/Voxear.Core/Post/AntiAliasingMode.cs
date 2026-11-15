namespace Voxear;

/// <summary>How edges are smoothed.</summary>
public enum AntiAliasingMode
{
	/// <summary>Not at all.</summary>
	Off,

	/// <summary>Fast and single-pass, slightly soft: for low-end GPUs.</summary>
	Fxaa,

	/// <summary>Temporal: smooth edges and stable detail. Reflections and render scale need it.</summary>
	Taa,
}
