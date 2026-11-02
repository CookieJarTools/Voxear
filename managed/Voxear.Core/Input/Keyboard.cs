using Voxear.Interop;

namespace Voxear;

/// <summary>
/// The keyboard as it is this frame, key by key. Games usually ask Input about actions instead, such as
/// <c>Input.Pressed("Jump")</c>, so players can rebind them; Keyboard is for keys that are always those keys, such
/// as a debug key.
/// </summary>
public static class Keyboard
{
	/// <summary>Whether the key is held down.</summary>
	public static bool IsDown(Key key)
	{
		return (Read(key) & NativeInput.ControlDown) != 0;
	}

	/// <summary>Whether the key went down this frame.</summary>
	public static bool WasPressed(Key key)
	{
		return (Read(key) & NativeInput.ControlPressed) != 0;
	}

	/// <summary>Whether the key came up this frame.</summary>
	public static bool WasReleased(Key key)
	{
		return (Read(key) & NativeInput.ControlReleased) != 0;
	}

	/// <summary>A key's path, as bindings name it: <c>Keyboard/W</c>, <c>Keyboard/1</c>.</summary>
	public static string Path(Key key)
	{
		var name = key.ToString();
		if (name.StartsWith("Digit", System.StringComparison.Ordinal))
		{
			name = name.Substring("Digit".Length);
		}
		return "Keyboard/" + name;
	}

	private static int Read(Key key)
	{
		var flags = NativeInput.Control(Path(key), out _);
		return flags < 0 ? 0 : flags;
	}
}
