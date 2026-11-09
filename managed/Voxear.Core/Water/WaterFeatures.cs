using System;

namespace Voxear;

/// <summary>
/// The sea's costlier looks, each of which can be turned off through <see cref="Water.Features"/> or the Ocean's
/// Quality settings in the Inspector.
/// </summary>
[Flags]
public enum WaterFeatures
{
	None = 0,

	/// <summary>Waves calming in shallow water, the sea stopping at the land, and its surf.</summary>
	Shores = 1 << 0,

	/// <summary>Foam kept from frame to frame: trails behind breaking crests, and surf that drifts and ages.</summary>
	FoamTrails = 1 << 1,

	/// <summary>Foam standing up off the water near the camera, its bubbles shading each other.</summary>
	Foam3D = 1 << 2,

	/// <summary>Light bending as it goes into the water, so what is under it shows shifted by the waves.</summary>
	LightBending = 1 << 3,

	/// <summary>Shafts of sunlight seen from under the water.</summary>
	LightShafts = 1 << 4,

	/// <summary>Drops and a film of water on the camera's lens after it comes up out of the water, or a wave falls away past it.</summary>
	WetLens = 1 << 5,

	/// <summary>The two finest bands of waves, ripples from a few centimetres to a metre long.</summary>
	FineRipples = 1 << 6,

	/// <summary>Specks drifting in the water round the camera, bubbles rising from below, and soft motes by the lens.</summary>
	Specks = 1 << 7,

	/// <summary>All of them, as an Ocean starts.</summary>
	All = (1 << 8) - 1,
}
