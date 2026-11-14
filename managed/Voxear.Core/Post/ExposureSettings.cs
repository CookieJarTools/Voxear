using System.Text.Json.Nodes;

namespace Voxear;

/// <summary>How bright the picture is, and how the camera adjusts to light and dark.</summary>
public sealed class ExposureSettings
{
	/// <summary>Adapts to the scene's brightness, as an eye does.</summary>
	public bool Auto = true;

	/// <summary>Brighter or darker than metered, in stops.</summary>
	public float Compensation = 0.5f;

	/// <summary>The most auto exposure may darken the picture, in stops.</summary>
	public float MinEv = -3f;

	/// <summary>The most auto exposure may brighten the picture, in stops.</summary>
	public float MaxEv = 1f;

	/// <summary>How fast it adapts towards brighter scenes, in stops a second.</summary>
	public float SpeedUp = 3f;

	/// <summary>How fast it adapts towards darker scenes, in stops a second.</summary>
	public float SpeedDown = 1f;

	/// <summary>How dark a dark scene stays: 1 as the eye sees, 0 brightens it as far as a daylit one.</summary>
	public float Night = 1f;

	/// <summary>The low percentile of the picture's brightness exposure is set by.</summary>
	public float LowPercent = 70f;

	/// <summary>The high percentile of the picture's brightness exposure is set by.</summary>
	public float HighPercent = 95f;

	internal void Read(JsonObject from)
	{
		PostJson.Read(from, "auto", ref Auto);
		PostJson.Read(from, "compensation", ref Compensation);
		PostJson.Read(from, "min_ev", ref MinEv);
		PostJson.Read(from, "max_ev", ref MaxEv);
		PostJson.Read(from, "speed_up", ref SpeedUp);
		PostJson.Read(from, "speed_down", ref SpeedDown);
		PostJson.Read(from, "night", ref Night);
		PostJson.Read(from, "low_percent", ref LowPercent);
		PostJson.Read(from, "high_percent", ref HighPercent);
	}

	internal void Write(JsonObject to)
	{
		to["auto"] = Auto;
		to["compensation"] = Compensation;
		to["min_ev"] = MinEv;
		to["max_ev"] = MaxEv;
		to["speed_up"] = SpeedUp;
		to["speed_down"] = SpeedDown;
		to["night"] = Night;
		to["low_percent"] = LowPercent;
		to["high_percent"] = HighPercent;
	}
}
