namespace Voxear;

/// <summary>How many rays ray-traced audio traces at most, and how many times each bounces.</summary>
public enum RayQuality
{
	/// <summary>256 rays from the listener a frame, bounced 6 times, and 4 across a volumetric source.</summary>
	Low,

	/// <summary>1024 rays, bounced 10 times, and 8 across a volumetric source: the default.</summary>
	Medium,

	/// <summary>4096 rays, bounced 16 times, and 16 across a volumetric source.</summary>
	High,
}
