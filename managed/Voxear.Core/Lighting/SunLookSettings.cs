using System.Text.Json.Nodes;

namespace Voxear;

/// <summary>
/// How the sun looks over the level's, as Lighting.SunLook reads and changes it: each setting left null is the level's
/// own. Colours are sRGB. How its shadows are drawn is Lighting.Shadows.
/// </summary>
public sealed class SunLookSettings
{
	/// <summary>Tints the sun to the colour of this temperature, in kelvin: 1900 for candlelight, 6500 for daylight.</summary>
	public float? Temperature;

	/// <summary>Tints the sun's colour, over its temperature's.</summary>
	public Color? Tint;

	/// <summary>Multiplies the sun's intensity.</summary>
	public float? Strength;

	/// <summary>How wide the sun looks, in degrees, for softer shadows, where the light keeps the default.</summary>
	public float? Softness;

	/// <summary>The colour its shadows take, where the light has none of its own.</summary>
	public Color? ShadowTint;

	/// <summary>Leaves over the level: the sun shines through a dappled canopy repeating every so many metres; 0 for none.</summary>
	public float? Canopy;

	/// <summary>Whether it changes anything.</summary>
	public bool IsSet => Temperature is not null || Tint is not null || Strength is not null || Softness is not null
		|| ShadowTint is not null || Canopy is not null;

	internal static SunLookSettings FromJson(string json)
	{
		var settings = new SunLookSettings();
		if (JsonNode.Parse(json) is not JsonObject read)
		{
			return settings;
		}
		settings.Temperature = Number(read, "temperature");
		PostJson.Read(read, "tint", ref settings.Tint);
		settings.Strength = Number(read, "strength");
		settings.Softness = Number(read, "softness");
		PostJson.Read(read, "shadow_tint", ref settings.ShadowTint);
		settings.Canopy = Number(read, "canopy");
		return settings;
	}

	internal SunLookSettings Copy()
	{
		return (SunLookSettings)MemberwiseClone();
	}

	internal JsonObject ToNode()
	{
		var to = new JsonObject();
		Put(to, "temperature", Temperature);
		if (Tint is { } tint)
		{
			to["tint"] = PostJson.Of(tint);
		}
		Put(to, "strength", Strength);
		Put(to, "softness", Softness);
		if (ShadowTint is { } shadow)
		{
			to["shadow_tint"] = PostJson.Of(shadow);
		}
		Put(to, "canopy", Canopy);
		return to;
	}

	private static float? Number(JsonObject from, string key)
	{
		float value = 0f;
		if (!from.ContainsKey(key))
		{
			return null;
		}
		PostJson.Read(from, key, ref value);
		return value;
	}

	private static void Put(JsonObject to, string key, float? value)
	{
		if (value is { } set)
		{
			to[key] = set;
		}
	}
}
