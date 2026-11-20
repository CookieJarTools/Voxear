using System.Text.Json.Nodes;

namespace Voxear;

/// <summary>The water's trails behind what moves through it, as Project Settings ▸ Water Trails has them; changed with ProjectSettings.ChangeWaterTrails.</summary>
public sealed class WaterTrailSettings
{
	/// <summary>How finely they are kept; Off for none.</summary>
	public WaterTrailQuality Quality = WaterTrailQuality.Medium;

	/// <summary>How far across the square round the camera they are kept in is, in metres.</summary>
	public float Area = 64f;

	/// <summary>How they look.</summary>
	public WaterTrailLook Look = WaterTrailLook.MatchWater;

	internal static WaterTrailSettings Read(JsonObject from)
	{
		var settings = new WaterTrailSettings();
		settings.Quality = SettingsJson.ReadChoice(from, "quality", settings.Quality);
		settings.Area = SettingsJson.Read(from, "area", settings.Area);
		settings.Look = SettingsJson.ReadChoice(from, "look", settings.Look);
		return settings;
	}

	internal JsonObject ToNode()
	{
		return new JsonObject
		{
			["quality"] = PostJson.Of(Quality),
			["area"] = Area,
			["look"] = PostJson.Of(Look),
		};
	}
}
