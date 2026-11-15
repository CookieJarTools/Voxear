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

	internal void Read(JsonObject from)
	{
		PostJson.Read(from, "enabled", ref Enabled);
		PostJson.Read(from, "strength", ref Strength);
		PostJson.Read(from, "probe_spacing", ref ProbeSpacing);
	}

	internal void Write(JsonObject to)
	{
		to["enabled"] = Enabled;
		to["strength"] = Strength;
		to["probe_spacing"] = ProbeSpacing;
	}
}
