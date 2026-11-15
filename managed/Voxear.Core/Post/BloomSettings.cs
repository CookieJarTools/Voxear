using System.Text.Json.Nodes;

namespace Voxear;

/// <summary>The soft glow round bright lights and highlights.</summary>
public sealed class BloomSettings
{
	/// <summary>Whether it is on.</summary>
	public bool Enabled = true;

	/// <summary>How much of the picture is its blurred glow.</summary>
	public float Intensity = 0.04f;

	/// <summary>0 keeps the glow tight, 1 spreads it wide.</summary>
	public float Scatter = 0.7f;

	/// <summary>The glow's colour; white keeps each light's own.</summary>
	public Color Tint = Color.White;

	internal void Read(JsonObject from)
	{
		PostJson.Read(from, "enabled", ref Enabled);
		PostJson.Read(from, "intensity", ref Intensity);
		PostJson.Read(from, "scatter", ref Scatter);
		PostJson.Read(from, "tint", ref Tint);
	}

	internal void Write(JsonObject to)
	{
		to["enabled"] = Enabled;
		to["intensity"] = Intensity;
		to["scatter"] = Scatter;
		to["tint"] = PostJson.Of(Tint);
	}
}
