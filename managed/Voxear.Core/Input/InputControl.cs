using System;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// One control of a device, as a binding reads it: a key, a mouse button or axis, or a gamepad's button or stick. It
/// is made from the devices' own enums, so a control is never spelled out as text that could be mistyped:
/// <code>
/// InputControl jump = Key.Space;
/// var fire = InputControl.Mouse(MouseButton.Left);
/// var look = InputControl.Gamepad(GamepadAxis.RightStick);
/// </code>
/// Label says what the player's own keyboard or pad calls it, and Path is how the input actions file writes it.
/// </summary>
public readonly record struct InputControl
{
	private InputControl(ControlDevice device, string name)
	{
		Device = device;
		Name = name;
	}

	/// <summary>The device it is on.</summary>
	public ControlDevice Device { get; }

	/// <summary>How the input actions file writes it, such as <c>Keyboard/W</c>.</summary>
	public string Path => $"{Device}/{Name}";

	/// <summary>
	/// What the player's own keyboard or pad calls it: the key where Q is on a US keyboard is "A" on an AZERTY one,
	/// and the south button is "Cross" on a PlayStation pad.
	/// </summary>
	public string Label => NativeInput.Label(Path);

	internal string Name { get; }

	public static InputControl Keyboard(Key key)
	{
		var name = key.ToString();
		return new InputControl(ControlDevice.Keyboard, name.StartsWith("Digit", StringComparison.Ordinal) ? name.Substring(5) : name);
	}

	public static InputControl Mouse(MouseButton button)
	{
		return new InputControl(ControlDevice.Mouse, button.ToString());
	}

	public static InputControl Mouse(MouseAxis axis)
	{
		return new InputControl(ControlDevice.Mouse, axis.ToString());
	}

	public static InputControl Gamepad(GamepadButton button)
	{
		return new InputControl(ControlDevice.Gamepad, button.ToString());
	}

	public static InputControl Gamepad(GamepadAxis axis)
	{
		return new InputControl(ControlDevice.Gamepad, axis.ToString());
	}

	public static implicit operator InputControl(Key key)
	{
		return Keyboard(key);
	}

	public static implicit operator InputControl(MouseButton button)
	{
		return Mouse(button);
	}

	public static implicit operator InputControl(MouseAxis axis)
	{
		return Mouse(axis);
	}

	public static implicit operator InputControl(GamepadButton button)
	{
		return Gamepad(button);
	}

	public static implicit operator InputControl(GamepadAxis axis)
	{
		return Gamepad(axis);
	}

	public override string ToString()
	{
		return Path;
	}

	/// <summary>A control by the path the engine gives for it, or null for one on no device, as a binding not yet given one.</summary>
	internal static InputControl? FromPath(string? path)
	{
		var slash = path?.IndexOf('/') ?? -1;
		if (path == null || slash <= 0 || slash == path.Length - 1)
		{
			return null;
		}
		var known = Enum.TryParse<ControlDevice>(path.Substring(0, slash), out var device);
		return known ? new InputControl(device, path.Substring(slash + 1)) : null;
	}
}
