using System.Globalization;
using System.Text.Json.Nodes;

namespace Voxear;

/// <summary>How the sun's and lamps' shadows are drawn, as Lighting.Shadows reads and changes them.</summary>
public sealed class ShadowSettings
{
	/// <summary>How the sun's shadows are drawn.</summary>
	public SunShadows Sun = SunShadows.Virtual;

	/// <summary>How lamps' shadows are drawn.</summary>
	public LampShadows Lamps = LampShadows.Virtual;

	/// <summary>How far from the camera the sun casts shadows with Cascades, in metres, from 5 to 1000.</summary>
	public float Distance = 80f;

	/// <summary>How the four cascades share that distance: 1 keeps the nearest small and sharp, 0 shares it evenly.</summary>
	public float Split = 0.85f;

	internal static ShadowSettings FromJson(string json)
	{
		var settings = new ShadowSettings();
		if (JsonNode.Parse(json) is not JsonObject read)
		{
			return settings;
		}
		settings.Sun = read["sun"]?.GetValue<string>() == "cascades" ? SunShadows.Cascades : SunShadows.Virtual;
		settings.Lamps = read["lamps"]?.GetValue<string>() == "atlas" ? LampShadows.Atlas : LampShadows.Virtual;
		settings.Distance = read["distance"]?.GetValue<float>() ?? settings.Distance;
		settings.Split = read["split"]?.GetValue<float>() ?? settings.Split;
		return settings;
	}

	/// <summary>The settings that differ from <paramref name="before"/>, as the engine takes them.</summary>
	internal JsonObject ChangedFrom(ShadowSettings before)
	{
		var changed = new JsonObject();
		if (Sun != before.Sun)
		{
			changed["sun"] = Sun == SunShadows.Cascades ? "cascades" : "virtual";
		}
		if (Lamps != before.Lamps)
		{
			changed["lamps"] = Lamps == LampShadows.Atlas ? "atlas" : "virtual";
		}
		if (Distance != before.Distance)
		{
			changed["distance"] = Distance;
		}
		if (Split != before.Split)
		{
			changed["split"] = Split;
		}
		return changed;
	}

	public override string ToString()
	{
		return string.Create(CultureInfo.InvariantCulture, $"{Sun} sun, {Lamps} lamps, {Distance} m, split {Split}");
	}
}
