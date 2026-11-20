using System.Text.Json.Nodes;

namespace Voxear;

/// <summary>What each area of ground costs to walk, as so many times plain ground, as Project Settings ▸ Navigation has it; changed with ProjectSettings.ChangeNavigationCosts for ways searched from then on.</summary>
public sealed class NavigationCosts
{
	/// <summary>A road, sought out.</summary>
	public float Road = 0.6f;

	/// <summary>Water a block deep, waded.</summary>
	public float Water = 3f;

	/// <summary>Water too deep to stand in, swum.</summary>
	public float Swim = 4f;

	/// <summary>A shut door, opened.</summary>
	public float Door = 2f;

	/// <summary>The project's own first area.</summary>
	public float Custom1 = 1f;

	/// <summary>The project's own second area.</summary>
	public float Custom2 = 1f;

	/// <summary>The project's own third area.</summary>
	public float Custom3 = 1f;

	/// <summary>The project's own fourth area.</summary>
	public float Custom4 = 1f;

	internal static NavigationCosts Read(JsonObject from)
	{
		var settings = new NavigationCosts();
		settings.Road = SettingsJson.Read(from, "road_cost", settings.Road);
		settings.Water = SettingsJson.Read(from, "water_cost", settings.Water);
		settings.Swim = SettingsJson.Read(from, "swim_cost", settings.Swim);
		settings.Door = SettingsJson.Read(from, "door_cost", settings.Door);
		settings.Custom1 = SettingsJson.Read(from, "custom_1_cost", settings.Custom1);
		settings.Custom2 = SettingsJson.Read(from, "custom_2_cost", settings.Custom2);
		settings.Custom3 = SettingsJson.Read(from, "custom_3_cost", settings.Custom3);
		settings.Custom4 = SettingsJson.Read(from, "custom_4_cost", settings.Custom4);
		return settings;
	}

	internal JsonObject ToNode()
	{
		return new JsonObject
		{
			["road_cost"] = Road,
			["water_cost"] = Water,
			["swim_cost"] = Swim,
			["door_cost"] = Door,
			["custom_1_cost"] = Custom1,
			["custom_2_cost"] = Custom2,
			["custom_3_cost"] = Custom3,
			["custom_4_cost"] = Custom4,
		};
	}
}
