using System.Text.Json.Nodes;

namespace Voxear;

/// <summary>How the game steps time, as Project Settings ▸ Time has it: the steps fixed once the game has started.</summary>
public sealed class TimeStepSettings
{
	/// <summary>How long each fixed step is, in seconds: 0.02 for 50 a second.</summary>
	public float FixedStep { get; private set; } = 0.02f;

	/// <summary>The longest a frame counts as, in seconds, however long it took.</summary>
	public float MaxStep { get; private set; } = 0.25f;

	/// <summary>The most fixed steps a frame takes to catch up.</summary>
	public int MaxFixedSteps { get; private set; } = 5;

	/// <summary>How fast the game's time goes, as it is now: Time.TimeScale changes it.</summary>
	public float TimeScale { get; private set; } = 1f;

	internal static TimeStepSettings Read(JsonObject from)
	{
		var settings = new TimeStepSettings();
		settings.FixedStep = SettingsJson.Read(from, "fixed_step", settings.FixedStep);
		settings.MaxStep = SettingsJson.Read(from, "max_step", settings.MaxStep);
		settings.MaxFixedSteps = SettingsJson.Read(from, "max_fixed_steps", settings.MaxFixedSteps);
		settings.TimeScale = SettingsJson.Read(from, "time_scale", settings.TimeScale);
		return settings;
	}
}
