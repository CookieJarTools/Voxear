using System;
using System.Text.Json.Nodes;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// The project's settings the game runs by, as Project Settings has them. Each section is a copy. Portals, water
/// trails, navigation costs and the input's tuning can be changed while the game runs, as an options menu does, from
/// the next frame, until the game starts again, and the time scale with Time.TimeScale; the time steps, the floating
/// origin, the quality budgets, the graphics and the network are fixed once it has started, and read only.
/// <code>
/// // A slow machine: Portals show their far side from nearer, and the water keeps no trails.
/// ProjectSettings.ChangePortals(portals =&gt; portals.ViewDistance = 48f);
/// ProjectSettings.ChangeWaterTrails(trails =&gt; trails.Quality = WaterTrailQuality.Off);
///
/// var step = ProjectSettings.Time.FixedStep;
/// </code>
/// </summary>
public static class ProjectSettings
{
	/// <summary>How the game steps time.</summary>
	public static TimeStepSettings Time => TimeStepSettings.Read(Section("time"));

	/// <summary>When and how far the world's middle moves with the viewer.</summary>
	public static FloatingOriginSettings FloatingOrigin => FloatingOriginSettings.Read(Section("floating_origin"));

	/// <summary>What the renderer keeps room for.</summary>
	public static QualityBudgets Quality => QualityBudgets.Read(Section("quality"));

	/// <summary>What the game asked of the GPU as it started.</summary>
	public static GraphicsSettings Graphics => GraphicsSettings.Read(Section("graphics"));

	/// <summary>How input actions read buttons and sticks. A copy: changing it changes nothing until it goes to ChangeInput.</summary>
	public static InputTuning Input => InputTuning.Read(Section("input"));

	/// <summary>How a networked game is played.</summary>
	public static NetworkSettings Network => NetworkSettings.Read(Section("network"));

	/// <summary>What Portals may spend. A copy: changing it changes nothing until it goes to ChangePortals.</summary>
	public static PortalSettings Portals => PortalSettings.Read(Section("portals"));

	/// <summary>The water's trails. A copy: changing it changes nothing until it goes to ChangeWaterTrails.</summary>
	public static WaterTrailSettings WaterTrails => WaterTrailSettings.Read(Section("water_trails"));

	/// <summary>What each area of ground costs to walk. A copy: changing it changes nothing until it goes to ChangeNavigationCosts.</summary>
	public static NavigationCosts NavigationCosts => NavigationCosts.Read(Section("navigation"));

	/// <summary>Changes what Portals may spend, from the next frame.</summary>
	public static void ChangePortals(Action<PortalSettings> change)
	{
		var settings = Portals;
		change(settings);
		Change("portals", settings.ToNode());
	}

	/// <summary>Changes the water's trails, from the next frame.</summary>
	public static void ChangeWaterTrails(Action<WaterTrailSettings> change)
	{
		var settings = WaterTrails;
		change(settings);
		Change("water_trails", settings.ToNode());
	}

	/// <summary>Changes how input actions read buttons and sticks, from the next frame.</summary>
	public static void ChangeInput(Action<InputTuning> change)
	{
		var tuning = Input;
		change(tuning);
		Change("input", tuning.ToNode());
	}

	/// <summary>Sets how fast the game's time goes, from the next frame: see Time.TimeScale.</summary>
	internal static void ChangeTimeScale(float scale)
	{
		Change("time", new JsonObject { ["time_scale"] = scale });
	}

	/// <summary>Changes what each area of ground costs, for ways searched from then on.</summary>
	public static void ChangeNavigationCosts(Action<NavigationCosts> change)
	{
		var settings = NavigationCosts;
		change(settings);
		Change("navigation", settings.ToNode());
	}

	private static JsonObject Section(string name)
	{
		var json = NativePost.Call(NativePost.ProjectSection, name);
		return json is not null && JsonNode.Parse(json) is JsonObject read ? read : [];
	}

	private static void Change(string section, JsonObject fields)
	{
		NativePost.Call(NativePost.ProjectChange, new JsonObject { ["section"] = section, ["fields"] = fields }.ToJsonString());
	}
}
