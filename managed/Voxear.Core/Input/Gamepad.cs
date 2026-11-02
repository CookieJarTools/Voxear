using System.Numerics;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// Gamepads as they are this frame: any pad connected counts. Games usually ask Input about actions instead, such
/// as <c>Input.Pressed("Jump")</c>, which the keyboard drives too.
/// </summary>
public static class Gamepad
{
	/// <summary>The left stick, each way from -1 to 1, up being positive.</summary>
	public static Vector2 LeftStick => Read("Gamepad/LeftStick");

	/// <summary>The right stick, each way from -1 to 1, up being positive.</summary>
	public static Vector2 RightStick => Read("Gamepad/RightStick");

	/// <summary>How far the left trigger is pulled, from 0 to 1.</summary>
	public static float LeftTrigger => Read("Gamepad/LeftTrigger").X;

	/// <summary>How far the right trigger is pulled, from 0 to 1.</summary>
	public static float RightTrigger => Read("Gamepad/RightTrigger").X;

	/// <summary>Whether the button is held down.</summary>
	public static bool IsDown(GamepadButton button)
	{
		return Read("Gamepad/" + button).X >= 0.5f;
	}

	private static Vector2 Read(string path)
	{
		NativeInput.Control(path, out var value);
		return value;
	}
}
