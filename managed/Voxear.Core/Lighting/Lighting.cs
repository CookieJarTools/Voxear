using System;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// The level's lighting: its sky, how its sun looks and its post-processing. A scene names the lighting settings it
/// loads with; Apply lights the level with others in place of the scenes' own, sky and post-processing and all, as
/// for a storm rolling in or a cave's gloom, until the level changes.
/// <code>
/// public LightingSettings Storm;
///
/// protected override void OnTriggerEnter(Collider other)
/// {
///     Lighting.Apply(Storm);
/// }
/// </code>
/// </summary>
public static class Lighting
{
	/// <summary>
	/// Lights the level with lighting settings from the next frame. Settings that cannot be read are reported in the
	/// Console, and the lighting stays as it was.
	/// </summary>
	public static void Apply(LightingSettings settings)
	{
		if (!settings.IsSet)
		{
			throw new ArgumentException("No lighting settings to apply.", nameof(settings));
		}

		Native.ApplyLighting(settings.Reference);
	}
}
