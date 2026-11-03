namespace Voxear;

/// <summary>
/// A gamepad's sticks as bindings read them: a whole stick as a vector, one axis of it, or one half of an axis, as a
/// stick pushed up can press a button action.
/// </summary>
public enum GamepadAxis
{
	LeftStick,
	LeftStickX,
	LeftStickY,
	LeftStickUp,
	LeftStickDown,
	LeftStickLeft,
	LeftStickRight,
	RightStick,
	RightStickX,
	RightStickY,
	RightStickUp,
	RightStickDown,
	RightStickLeft,
	RightStickRight,
}
