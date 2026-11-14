using System.Text.Json.Nodes;

namespace Voxear;

/// <summary>Blur of what moves fast, as a film camera's.</summary>
public sealed class MotionBlurSettings
{
	/// <summary>Whether it is on.</summary>
	public bool Enabled = false;

	/// <summary>The share of the frame's motion that blurs.</summary>
	public float Intensity = 0.5f;

	internal void Read(JsonObject from)
	{
		PostJson.Read(from, "enabled", ref Enabled);
		PostJson.Read(from, "intensity", ref Intensity);
	}

	internal void Write(JsonObject to)
	{
		to["enabled"] = Enabled;
		to["intensity"] = Intensity;
	}
}
