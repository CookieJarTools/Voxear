using System.Text.Json.Nodes;

namespace Voxear;

/// <summary>The sky's settings, as Lighting.Sky reads and changes them. Colours are sRGB.</summary>
public sealed class SkySettings
{
	/// <summary>A gradient of the colours below, or the Earth's air lit by the sun.</summary>
	public SkyMode Mode = SkyMode.Gradient;

	/// <summary>A gradient sky's colour overhead.</summary>
	public Color Zenith = new(0.517f, 0.701f, 0.931f);

	/// <summary>A gradient sky's colour round the horizon.</summary>
	public Color Horizon = new(0.809f, 0.865f, 0.936f);

	/// <summary>A gradient sky's colour below the horizon.</summary>
	public Color Ground = new(0.506f, 0.485f, 0.461f);

	/// <summary>Scales the ambient light the sky gives the scene.</summary>
	public float Ambient = 1f;

	/// <summary>How bright the stars are, 1 as the night sky shows them. They show only when the sky is dark.</summary>
	public float Stars = 1f;

	/// <summary>The atmosphere's haze: 1 as on a clear day, more for a hazy, golden or smoggy one.</summary>
	public float Haze = 1f;

	/// <summary>The colour the haze scatters: white for moisture's grey, warmer for dust.</summary>
	public Color HazeTint = new(1f, 1f, 1f);

	/// <summary>How much the haze glows round the sun rather than evenly, from 0 to 0.95.</summary>
	public float HazeGlow = 0.8f;

	/// <summary>How quickly haze builds up with distance: things count as this many times as far for it.</summary>
	public float DistanceHaze = 10f;

	/// <summary>The sun's disc across the sky, in degrees.</summary>
	public float SunSize = 0.5357f;

	/// <summary>How far the sky's picture, where the level's sky has one, is turned round, in degrees.</summary>
	public float PictureTurn = 0f;

	/// <summary>The clouds in it.</summary>
	public CloudSettings Clouds = new();

	internal static SkySettings FromJson(string json)
	{
		var settings = new SkySettings();
		if (JsonNode.Parse(json) is JsonObject read)
		{
			settings.Read(read);
		}
		return settings;
	}

	internal void Read(JsonObject from)
	{
		PostJson.ReadChoice(from, "mode", ref Mode);
		PostJson.Read(from, "zenith", ref Zenith);
		PostJson.Read(from, "horizon", ref Horizon);
		PostJson.Read(from, "ground", ref Ground);
		PostJson.Read(from, "ambient", ref Ambient);
		PostJson.Read(from, "stars", ref Stars);
		PostJson.Read(from, "haze", ref Haze);
		PostJson.Read(from, "haze_tint", ref HazeTint);
		PostJson.Read(from, "haze_glow", ref HazeGlow);
		PostJson.Read(from, "distance_haze", ref DistanceHaze);
		PostJson.Read(from, "sun_size", ref SunSize);
		PostJson.Read(from, "picture_turn", ref PictureTurn);
		Clouds.Read(PostJson.Section(from, "clouds"));
	}

	internal JsonObject ToNode()
	{
		return new JsonObject
		{
			["mode"] = PostJson.Of(Mode),
			["zenith"] = PostJson.Of(Zenith),
			["horizon"] = PostJson.Of(Horizon),
			["ground"] = PostJson.Of(Ground),
			["ambient"] = Ambient,
			["stars"] = Stars,
			["haze"] = Haze,
			["haze_tint"] = PostJson.Of(HazeTint),
			["haze_glow"] = HazeGlow,
			["distance_haze"] = DistanceHaze,
			["sun_size"] = SunSize,
			["picture_turn"] = PictureTurn,
			["clouds"] = PostJson.Node(Clouds.Write),
		};
	}
}
