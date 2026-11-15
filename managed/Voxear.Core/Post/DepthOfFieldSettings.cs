using System.Text.Json.Nodes;

namespace Voxear;

/// <summary>Focus blur, as a camera lens's.</summary>
public sealed class DepthOfFieldSettings
{
	/// <summary>Whether it is on.</summary>
	public bool Enabled = false;

	/// <summary>How far away, in metres, things are in sharp focus.</summary>
	public float FocusDistance = 10f;

	/// <summary>The lens's f-stop: smaller blurs more.</summary>
	public float Aperture = 2.8f;

	/// <summary>The largest blur, in pixels at 1080p.</summary>
	public float MaxBlur = 12f;

	/// <summary>Whether what lies beyond the focus blurs too. Off blurs only the near foreground.</summary>
	public bool Far = true;

	internal void Read(JsonObject from)
	{
		PostJson.Read(from, "enabled", ref Enabled);
		PostJson.Read(from, "focus_distance", ref FocusDistance);
		PostJson.Read(from, "aperture", ref Aperture);
		PostJson.Read(from, "max_blur", ref MaxBlur);
		PostJson.Read(from, "far", ref Far);
	}

	internal void Write(JsonObject to)
	{
		to["enabled"] = Enabled;
		to["focus_distance"] = FocusDistance;
		to["aperture"] = Aperture;
		to["max_blur"] = MaxBlur;
		to["far"] = Far;
	}
}
