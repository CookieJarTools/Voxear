using System;
using System.Text.Json.Nodes;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// The level's lighting: its sky, how its sun looks and its post-processing. A scene names the lighting settings it
/// loads with; Apply lights the level with others in place of the scenes' own, sky and post-processing and all, as
/// for a storm rolling in or a cave's gloom, until the level changes. Shadows, Sky and SunLook read and change how
/// shadows are drawn, the sky and how the sun looks over the level's, as an options menu or a moment of the game
/// does, and are kept through level changes until reset.
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
///
/// // A storm gathering: cloud over, and a dimmer, cooler sun.
/// Lighting.ChangeSky(sky =&gt; sky.Clouds.Cover = 0.9f);
/// Lighting.ChangeSun(sun =&gt;
/// {
///     sun.Temperature = 7500f;
///     sun.Strength = 0.4f;
/// });
/// </code>
/// </summary>
public static class Lighting
{
	private static ShadowSettings? awayShadows;
	private static JsonObject awaySky = [];
	private static SunLookSettings awaySun = new();

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
	/// The sky as a frame draws it now, weather aside: the level's, a worn look pack's, and what scripts changed over
	/// them. A copy: changing it changes nothing until it goes to ChangeSky.
	/// </summary>
	public static SkySettings Sky
	{
		get
		{
			if (NativePost.Call(NativePost.SkyInEffect, string.Empty) is { } json)
			{
				return SkySettings.FromJson(json);
			}
			var settings = new SkySettings();
			settings.Read(awaySky);
			return settings;
		}
	}

	/// <summary>
	/// Changes the sky over the level's: each setting the change sets to other than it is now, the rest left to the
	/// level. Kept through level changes until ResetSky.
	/// </summary>
	public static void ChangeSky(Action<SkySettings> change)
	{
		var settings = Sky;
		var before = settings.ToNode();
		change(settings);
		var changed = new JsonObject();
		PostJson.Changed(before, settings.ToNode(), changed);
		if (changed.Count == 0)
		{
			return;
		}
		if (!NativePost.IsConnected)
		{
			PostJson.Changed([], changed, awaySky);
			return;
		}
		NativePost.Call(NativePost.SkySet, changed.ToJsonString());
	}

	/// <summary>Takes off what scripts changed of the sky: the level's own shows again.</summary>
	public static void ResetSky()
	{
		awaySky = [];
		NativePost.Call(NativePost.SkyReset, string.Empty);
	}

	/// <summary>
	/// How scripts have the sun look over the level's: each setting null is the level's own. A copy: changing it
	/// changes nothing until it goes to ChangeSun.
	/// </summary>
	public static SunLookSettings SunLook
	{
		get
		{
			if (NativePost.Call(NativePost.SunLook, string.Empty) is { } json)
			{
				return SunLookSettings.FromJson(json);
			}
			return awaySun.Copy();
		}
	}

	/// <summary>
	/// Changes how the sun looks over the level's; a setting set back to null is the level's again. Kept through level
	/// changes until ResetSun.
	/// </summary>
	public static void ChangeSun(Action<SunLookSettings> change)
	{
		var settings = SunLook;
		change(settings);
		if (!NativePost.IsConnected)
		{
			awaySun = settings;
			return;
		}
		NativePost.Call(NativePost.SunLookSet, settings.ToNode().ToJsonString());
	}

	/// <summary>Takes off what scripts changed of how the sun looks: the level's own shows again.</summary>
	public static void ResetSun()
	{
		awaySun = new SunLookSettings();
		NativePost.Call(NativePost.SunLookReset, string.Empty);
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
