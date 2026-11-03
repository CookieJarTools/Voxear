namespace Voxear;

/// <summary>
/// A gamepad's buttons, by where they sit rather than what they say, so South is A on an Xbox pad and Cross on a
/// PlayStation one. Bindings name them the same way, such as <c>Gamepad/South</c>.
/// </summary>
public enum GamepadButton
{
	South,
	East,
	West,
	North,
	LeftShoulder,
	RightShoulder,
	LeftTrigger,
	RightTrigger,
	Select,
	Start,
	LeftStickPress,
	RightStickPress,
	DpadUp,
	DpadDown,
	DpadLeft,
	DpadRight,
	/// <summary>The button in the middle: Xbox's guide button, PlayStation's PS button, the Switch's Home.</summary>
	Mode,
}
