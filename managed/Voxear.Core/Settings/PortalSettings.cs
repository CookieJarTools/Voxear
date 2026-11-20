using System.Text.Json.Nodes;

namespace Voxear;

/// <summary>What Portals may spend, as Project Settings ▸ Portals has it; changed with ProjectSettings.ChangePortals.</summary>
public sealed class PortalSettings
{
	/// <summary>How many far sides a view draws at most, the nearest first.</summary>
	public int Views = 4;

	/// <summary>How deep Portals seen through Portals are drawn: 2 draws the far side of one seen through another.</summary>
	public int Depth = 1;

	/// <summary>How near a far side's opening the sun casts shadows on it, in metres.</summary>
	public float ShadowDistance = 30f;

	/// <summary>How far round a far side a world without end makes its own blocks, in metres.</summary>
	public float FarSideDistance = 96f;

	/// <summary>Whether the water of a far side's worlds of blocks is drawn.</summary>
	public bool Water = true;

	/// <summary>How far ahead a far side is drawn, in metres; 0 for as far as the world is.</summary>
	public float DrawDistance = 0f;

	/// <summary>How far off a Portal without its own still shows its far side, in metres.</summary>
	public float ViewDistance = 96f;

	/// <summary>How near a Portal into a dimension loaded for it the dimension is loaded, in metres.</summary>
	public float DimensionDistance = 24f;

	internal static PortalSettings Read(JsonObject from)
	{
		var settings = new PortalSettings();
		settings.Views = SettingsJson.Read(from, "views", settings.Views);
		settings.Depth = SettingsJson.Read(from, "depth", settings.Depth);
		settings.ShadowDistance = SettingsJson.Read(from, "shadow_distance", settings.ShadowDistance);
		settings.FarSideDistance = SettingsJson.Read(from, "far_side_distance", settings.FarSideDistance);
		settings.Water = SettingsJson.Read(from, "water", settings.Water);
		settings.DrawDistance = SettingsJson.Read(from, "draw_distance", settings.DrawDistance);
		settings.ViewDistance = SettingsJson.Read(from, "view_distance", settings.ViewDistance);
		settings.DimensionDistance = SettingsJson.Read(from, "dimension_distance", settings.DimensionDistance);
		return settings;
	}

	internal JsonObject ToNode()
	{
		return new JsonObject
		{
			["views"] = Views,
			["depth"] = Depth,
			["shadow_distance"] = ShadowDistance,
			["far_side_distance"] = FarSideDistance,
			["water"] = Water,
			["draw_distance"] = DrawDistance,
			["view_distance"] = ViewDistance,
			["dimension_distance"] = DimensionDistance,
		};
	}
}
