using System.Text.Json.Nodes;

namespace Voxear;

/// <summary>When and how far the world's middle moves with the viewer, as Project Settings ▸ Floating Origin has it: fixed once the game has started.</summary>
public sealed class FloatingOriginSettings
{
	/// <summary>When the middle moves.</summary>
	public FloatingOriginMode Mode { get; private set; } = FloatingOriginMode.Off;

	/// <summary>How far the viewer goes from the middle before it moves, in metres.</summary>
	public float Distance { get; private set; } = 2048f;

	/// <summary>What the middle moves by, in whole metres.</summary>
	public int Step { get; private set; } = 1024;

	internal static FloatingOriginSettings Read(JsonObject from)
	{
		var settings = new FloatingOriginSettings();
		settings.Mode = SettingsJson.ReadChoice(from, "mode", settings.Mode);
		settings.Distance = SettingsJson.Read(from, "distance", settings.Distance);
		settings.Step = SettingsJson.Read(from, "step", settings.Step);
		return settings;
	}
}
