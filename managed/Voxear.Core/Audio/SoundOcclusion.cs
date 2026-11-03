namespace Voxear;

/// <summary>How an AudioSource tells whether walls between it and the listener stop its sound.</summary>
public enum SoundOcclusion
{
	/// <summary>Walls do not stop it.</summary>
	Off,

	/// <summary>One ray from its middle to the listener: in the way or not.</summary>
	Raycast,

	/// <summary>Rays across a sphere OcclusionRadius round it, so a source half behind a corner is half heard.</summary>
	Volumetric,
}
