using System;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// The level's lighting: its sky, how its sun looks and its post-processing. A scene names the lighting settings it
/// loads with; Apply lights the level with others in place of the scenes' own, sky and post-processing and all, as
/// for a storm rolling in or a cave's gloom, until the level changes. Shadows reads and changes how shadows are
/// drawn over the level's, as a graphics options menu does.
/// <code>
/// public LightingSettings Storm;
///
/// protected override void OnTriggerEnter(Collider other)
/// {
///     Lighting.Apply(Storm);
/// }
///
/// // From an options menu: shadow maps drawn whole each frame, out to 120 m.
/// Lighting.ChangeShadows(shadows =&gt;
/// {
///     shadows.Sun = SunShadows.Cascades;
///     shadows.Distance = 120f;
/// });
/// </code>
/// </summary>
public static class Lighting
{
	private static ShadowSettings? awayShadows;

	/// <summary>
	/// How shadows are drawn now: the level's lighting, a worn look or shader pack's, and what scripts changed over
	/// them. A copy: changing it changes nothing until it goes to ChangeShadows.
	/// </summary>
	public static ShadowSettings Shadows
	{
		get
		{
			if (NativePost.Call(NativePost.ShadowsInEffect, string.Empty) is { } json)
			{
				return ShadowSettings.FromJson(json);
			}
			return awayShadows ?? new ShadowSettings();
		}
	}

	/// <summary>
	/// Changes how shadows are drawn, over the level's: each setting the change sets to other than it is now, the rest
	/// left to the level. Kept through level changes until ResetShadows.
	/// </summary>
	public static void ChangeShadows(Action<ShadowSettings> change)
	{
		var settings = Shadows;
		var before = Shadows;
		change(settings);
		if (!NativePost.IsConnected)
		{
			awayShadows = settings;
			return;
		}
		var changed = settings.ChangedFrom(before);
		if (changed.Count > 0)
		{
			NativePost.Call(NativePost.ShadowsSet, changed.ToJsonString());
		}
	}

	/// <summary>Takes off what scripts changed of the shadows: the level's own show again.</summary>
	public static void ResetShadows()
	{
		awayShadows = null;
		NativePost.Call(NativePost.ShadowsReset, string.Empty);
	}

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
