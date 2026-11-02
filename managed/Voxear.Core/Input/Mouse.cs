using System.Numerics;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// The mouse as it is this frame: where it points, how far it moved, its wheel and its buttons. Games usually ask
/// Input about actions instead, such as <c>Input.Vector2("Look")</c>, which a gamepad's stick drives too.
/// </summary>
public static class Mouse
{
	/// <summary>Where the pointer is over the game, in pixels from its top left.</summary>
	public static Vector2 Position => Read("Mouse/Position");

	/// <summary>How far the mouse moved this frame, in pixels, right and down. It keeps counting while the cursor is locked.</summary>
	public static Vector2 Delta => Read("Mouse/Delta");

	/// <summary>How far the wheel turned this frame, in notches, right and up.</summary>
	public static Vector2 Scroll => Read("Mouse/Scroll");

	/// <summary>Whether the button is held down.</summary>
	public static bool IsDown(MouseButton button)
	{
		return (Flags(button) & NativeInput.ControlDown) != 0;
	}

	/// <summary>Whether the button went down this frame.</summary>
	public static bool WasPressed(MouseButton button)
	{
		return (Flags(button) & NativeInput.ControlPressed) != 0;
	}

	/// <summary>Whether the button came up this frame.</summary>
	public static bool WasReleased(MouseButton button)
	{
		return (Flags(button) & NativeInput.ControlReleased) != 0;
	}

	private static int Flags(MouseButton button)
	{
		var flags = NativeInput.Control("Mouse/" + button, out _);
		return flags < 0 ? 0 : flags;
	}

	private static Vector2 Read(string path)
	{
		NativeInput.Control(path, out var value);
		return value;
	}
}
