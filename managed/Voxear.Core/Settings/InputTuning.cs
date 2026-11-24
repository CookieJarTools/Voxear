using System.Text.Json.Nodes;

namespace Voxear;

/// <summary>
/// How every input action reads buttons and sticks, as Project Settings ▸ Input has it; changed with
/// ProjectSettings.ChangeInput, as an options menu's deadzone does.
/// </summary>
public sealed class InputTuning
{
	/// <summary>How far a button or trigger goes before it counts as pressed, from 0.01 to 1.</summary>
	public float PressPoint = 0.5f;

	/// <summary>A stick's travel read as nothing, where a binding gives no deadzone of its own.</summary>
	public float StickDeadZone = 0.125f;

	/// <summary>A stick's travel from which it reads as all the way.</summary>
	public float StickFullAt = 0.925f;

	internal static InputTuning Read(JsonObject from)
	{
		var tuning = new InputTuning();
		tuning.PressPoint = SettingsJson.Read(from, "press_point", tuning.PressPoint);
		if (from["stick_dead_zone"] is JsonArray { Count: 2 } zone)
		{
			tuning.StickDeadZone = zone[0]?.GetValue<float>() ?? tuning.StickDeadZone;
			tuning.StickFullAt = zone[1]?.GetValue<float>() ?? tuning.StickFullAt;
		}
		return tuning;
	}

	internal JsonObject ToNode()
	{
		return new JsonObject
		{
			["press_point"] = PressPoint,
			["stick_dead_zone"] = new JsonArray(StickDeadZone, StickFullAt),
		};
	}
}
