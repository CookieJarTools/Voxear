namespace Voxear;

/// <summary>What builds the full picture from the scene drawn smaller, with TAA.</summary>
public enum Upscaler
{
	/// <summary>Voxear's own temporal upscaler, on every GPU.</summary>
	Voxear,

	/// <summary>AMD FSR 3, where the GPU can run it; Voxear's own where it cannot.</summary>
	Fsr,

	/// <summary>NVIDIA DLSS: not built in, so Voxear's own stands in.</summary>
	Dlss,

	/// <summary>Intel XeSS: not built in, so Voxear's own stands in.</summary>
	Xess,
}
