using System.Text.Json.Nodes;

namespace Voxear;

/// <summary>Darkened corners, drawing the eye to the middle.</summary>
public sealed class VignetteSettings
{
	/// <summary>How dark the corners get.</summary>
	public float Intensity = 0.2f;

	/// <summary>How softly it falls off.</summary>
	public float Smoothness = 0.4f;

	/// <summary>1 for round, 0 for the screen's own shape.</summary>
	public float Roundness = 1f;

	internal void Read(JsonObject from)
	{
		PostJson.Read(from, "intensity", ref Intensity);
		PostJson.Read(from, "smoothness", ref Smoothness);
		PostJson.Read(from, "roundness", ref Roundness);
	}

	internal void Write(JsonObject to)
	{
		to["intensity"] = Intensity;
		to["smoothness"] = Smoothness;
		to["roundness"] = Roundness;
	}
}
