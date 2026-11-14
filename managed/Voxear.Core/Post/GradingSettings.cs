using System.Numerics;
using System.Text.Json.Nodes;

namespace Voxear;

/// <summary>The colour grade over the finished picture.</summary>
public sealed class GradingSettings
{
	/// <summary>White balance: negative is cooler, positive warmer.</summary>
	public float Temperature = 0f;

	/// <summary>Negative is greener, positive more magenta.</summary>
	public float Tint = 0f;

	/// <summary>Contrast in the grade, after the tonemapper.</summary>
	public float Contrast = 1f;

	/// <summary>Colour strength in the grade, after the tonemapper.</summary>
	public float Saturation = 1f;

	/// <summary>Lifts the shadows, per channel.</summary>
	public Vector3 Lift = Vector3.Zero;

	/// <summary>Moves the midtones, per channel.</summary>
	public Vector3 Gamma = Vector3.One;

	/// <summary>Scales the highlights, per channel.</summary>
	public Vector3 Gain = Vector3.One;

	/// <summary>A colour lookup table, applied last: a picture of N squares of N×N side by side.</summary>
	public Texture Lut = Texture.None;

	/// <summary>How much of the lookup table's grade shows, from 0 to 1.</summary>
	public float LutContribution = 1f;

	internal void Read(JsonObject from)
	{
		PostJson.Read(from, "temperature", ref Temperature);
		PostJson.Read(from, "tint", ref Tint);
		PostJson.Read(from, "contrast", ref Contrast);
		PostJson.Read(from, "saturation", ref Saturation);
		PostJson.Read(from, "lift", ref Lift);
		PostJson.Read(from, "gamma", ref Gamma);
		PostJson.Read(from, "gain", ref Gain);
		PostJson.Read(from, "lut", ref Lut);
		PostJson.Read(from, "lut_contribution", ref LutContribution);
	}

	internal void Write(JsonObject to)
	{
		to["temperature"] = Temperature;
		to["tint"] = Tint;
		to["contrast"] = Contrast;
		to["saturation"] = Saturation;
		to["lift"] = PostJson.Of(Lift);
		to["gamma"] = PostJson.Of(Gamma);
		to["gain"] = PostJson.Of(Gain);
		to["lut"] = PostJson.Of(Lut);
		to["lut_contribution"] = LutContribution;
	}
}
