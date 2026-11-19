using System.Text.Json.Nodes;

namespace Voxear;

/// <summary>Bounced light, from probes round the camera whose rays are traced against the scene.</summary>
public sealed class GlobalIlluminationSettings
{
	/// <summary>Whether it is on.</summary>
	public bool Enabled = false;

	/// <summary>How bright the bounced light is: 1 as measured.</summary>
	public float Strength = 1f;

	/// <summary>How far apart the probes round the camera are, in metres.</summary>
	public float ProbeSpacing = 1f;

	/// <summary>Where the bounced light comes from: the probes round the camera, the level's bake, or both.</summary>
	public GlobalIlluminationSource Source = GlobalIlluminationSource.Realtime;

	/// <summary>A bake's lightmap texels a metre of surface, before each Obj's own scale.</summary>
	public float TexelsPerMetre = 10f;

	/// <summary>How many times a bake's light bounces.</summary>
	public int Bounces = 3;

	/// <summary>The paths a bake traces from each lightmap texel; light probes trace twice as many.</summary>
	public int Samples = 256;

	/// <summary>Whether a bake keeps which way its light mostly comes from, so normal maps still show under it.</summary>
	public bool Directional = true;

	/// <summary>Whether a bake's noise is smoothed away.</summary>
	public bool Denoise = true;

	/// <summary>The most texels across a lightmap.</summary>
	public int LightmapSize = 1024;

	internal void Read(JsonObject from)
	{
		PostJson.Read(from, "enabled", ref Enabled);
		PostJson.Read(from, "strength", ref Strength);
		PostJson.Read(from, "probe_spacing", ref ProbeSpacing);
		PostJson.ReadChoice(from, "source", ref Source);
		PostJson.Read(from, "texels_per_metre", ref TexelsPerMetre);
		PostJson.Read(from, "bounces", ref Bounces);
		PostJson.Read(from, "samples", ref Samples);
		PostJson.Read(from, "directional", ref Directional);
		PostJson.Read(from, "denoise", ref Denoise);
		PostJson.Read(from, "lightmap_size", ref LightmapSize);
	}

	internal void Write(JsonObject to)
	{
		to["enabled"] = Enabled;
		to["strength"] = Strength;
		to["probe_spacing"] = ProbeSpacing;
		to["source"] = PostJson.Of(Source);
		to["texels_per_metre"] = TexelsPerMetre;
		to["bounces"] = Bounces;
		to["samples"] = Samples;
		to["directional"] = Directional;
		to["denoise"] = Denoise;
		to["lightmap_size"] = LightmapSize;
	}
}
