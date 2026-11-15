using System.Text.Json.Nodes;

namespace Voxear;

/// <summary>Height fog, and the light shafts that show in it.</summary>
public sealed class FogSettings
{
	/// <summary>Whether it is on.</summary>
	public bool Enabled = true;

	/// <summary>How thick it is: 0.01 is a light mist.</summary>
	public float Density = 0.004f;

	/// <summary>How fast it thins as it rises.</summary>
	public float HeightFalloff = 0.12f;

	/// <summary>Where it is densest, in metres.</summary>
	public float BaseHeight = 0f;

	/// <summary>Its colour; null for the sky's horizon colour.</summary>
	public Color? Color = null;

	/// <summary>How strongly it glows towards the sun.</summary>
	public float SunGlow = 0.5f;

	/// <summary>Lights it near the camera, so shadows cut shafts through it and lamps glow in it.</summary>
	public bool Volumetric = false;

	/// <summary>How far from the camera the shafts reach, in metres.</summary>
	public float VolumetricDistance = 64f;

	/// <summary>How much it glows looking towards a light, from -1 to 1.</summary>
	public float Anisotropy = 0.6f;

	internal void Read(JsonObject from)
	{
		PostJson.Read(from, "enabled", ref Enabled);
		PostJson.Read(from, "density", ref Density);
		PostJson.Read(from, "height_falloff", ref HeightFalloff);
		PostJson.Read(from, "base_height", ref BaseHeight);
		PostJson.Read(from, "color", ref Color);
		PostJson.Read(from, "sun_glow", ref SunGlow);
		PostJson.Read(from, "volumetric", ref Volumetric);
		PostJson.Read(from, "volumetric_distance", ref VolumetricDistance);
		PostJson.Read(from, "anisotropy", ref Anisotropy);
	}

	internal void Write(JsonObject to)
	{
		to["enabled"] = Enabled;
		to["density"] = Density;
		to["height_falloff"] = HeightFalloff;
		to["base_height"] = BaseHeight;
		to["color"] = PostJson.Of(Color);
		to["sun_glow"] = SunGlow;
		to["volumetric"] = Volumetric;
		to["volumetric_distance"] = VolumetricDistance;
		to["anisotropy"] = Anisotropy;
	}
}
