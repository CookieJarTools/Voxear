using System.Text.Json.Nodes;

namespace Voxear;

/// <summary>
/// The view breathing and wavering: the picture swells and sinks a little about its middle, slowly, and wavers as
/// though seen through heat, more towards its edges.
/// </summary>
public sealed class WarpSettings
{
	/// <summary>How much the picture swells and sinks, as a share of its size: 0.01 is felt more than seen.</summary>
	public float Breathing;

	/// <summary>How far it wavers, as a share of the picture's height: 0.003 is a faint shimmer.</summary>
	public float Waver;

	/// <summary>Breaths a second; the waver drifts four times as fast.</summary>
	public float Speed = 0.12f;

	internal void Read(JsonObject from)
	{
		PostJson.Read(from, "breathing", ref Breathing);
		PostJson.Read(from, "waver", ref Waver);
		PostJson.Read(from, "speed", ref Speed);
	}

	internal void Write(JsonObject to)
	{
		to["breathing"] = Breathing;
		to["waver"] = Waver;
		to["speed"] = Speed;
	}
}
