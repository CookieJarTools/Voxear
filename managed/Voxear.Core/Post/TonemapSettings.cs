using System.Text.Json.Nodes;

namespace Voxear;

/// <summary>How bright light is fitted to what a screen shows.</summary>
public sealed class TonemapSettings
{
	/// <summary>The curve.</summary>
	public Tonemapper Mapper = Tonemapper.Agx;

	/// <summary>AgX's contrast: 1 is its own.</summary>
	public float Contrast = 1.1f;

	/// <summary>AgX's saturation: 1 leaves colours as they are.</summary>
	public float Saturation = 1.15f;

	internal void Read(JsonObject from)
	{
		PostJson.ReadChoice(from, "mapper", ref Mapper);
		PostJson.Read(from, "contrast", ref Contrast);
		PostJson.Read(from, "saturation", ref Saturation);
	}

	internal void Write(JsonObject to)
	{
		to["mapper"] = PostJson.Of(Mapper);
		to["contrast"] = Contrast;
		to["saturation"] = Saturation;
	}
}
