using System.Text.Json.Nodes;

namespace Voxear;

/// <summary>Ambient occlusion: creases, corners and contacts darken, where less of the sky's light reaches. It shades only the sky's light, never the sun or lamps.</summary>
public sealed class AmbientOcclusionSettings
{
	/// <summary>Whether it is on.</summary>
	public bool Enabled = true;

	/// <summary>How far, in metres, a surface looks for what shades it.</summary>
	public float Radius = 0.75f;

	/// <summary>1 is as measured; higher darkens more, 0 leaves the light alone.</summary>
	public float Strength = 1f;

	/// <summary>The colour fully occluded light takes on. Black only darkens; a warm brown or a purple tints contact shadows.</summary>
	public Color Tint = Color.Black;

	/// <summary>How hard it looks for what shades each pixel.</summary>
	public AmbientOcclusionQuality Quality = AmbientOcclusionQuality.Medium;

	/// <summary>Searches every pixel rather than one of each 2x2 a frame: sharper in fine detail, about four times the cost.</summary>
	public bool FullResolution = false;

	internal void Read(JsonObject from)
	{
		PostJson.Read(from, "enabled", ref Enabled);
		PostJson.Read(from, "radius", ref Radius);
		PostJson.Read(from, "strength", ref Strength);
		PostJson.Read(from, "tint", ref Tint);
		PostJson.ReadChoice(from, "quality", ref Quality);
		PostJson.Read(from, "full_resolution", ref FullResolution);
	}

	internal void Write(JsonObject to)
	{
		to["enabled"] = Enabled;
		to["radius"] = Radius;
		to["strength"] = Strength;
		to["tint"] = PostJson.Of(Tint);
		to["quality"] = PostJson.Of(Quality);
		to["full_resolution"] = FullResolution;
	}
}
