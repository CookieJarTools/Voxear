using System;
using System.Numerics;
using System.Text;

namespace Voxear.Interop;

/// <summary>The engine's input, for Input and the devices: what each native call takes and gives, as C# has it.</summary>
internal static unsafe class NativeInput
{
	public const int TextPrompt = 0;
	public const int TextScheme = 1;
	public const int TextSaved = 2;
	public const int TextMaps = 3;
	public const int TextBindings = 4;
	public const int TextRebound = 5;
	public const int TextName = 6;
	public const int TextCategories = 7;
	public const int TextActions = 8;
	public const int TextPlayers = 9;
	public const int TextJoined = 10;

	public const int ChangeRebind = 0;
	public const int ChangeListen = 1;
	public const int ChangePoll = 2;
	public const int ChangeLoad = 3;
	public const int ChangeReset = 4;
	public const int ChangeMap = 5;
	public const int ChangeCancel = 6;
	public const int ChangeResetAction = 7;
	public const int ChangeJoining = 8;
	public const int ChangeRemovePlayer = 9;
	public const int ChangeMostPlayers = 10;

	/// <summary>The control bits a key or a mouse button reads as.</summary>
	public const int ControlDown = 1;
	public const int ControlPressed = 2;
	public const int ControlReleased = 4;

	/// <summary>An action's place by its name, or -1 when there is none.</summary>
	public static int Action(string name)
	{
		if (!Native.IsConnected)
		{
			return -1;
		}
		var bytes = Encoding.UTF8.GetBytes(name);
		fixed (byte* written = bytes)
		{
			return Native.Api->InputAction(written, bytes.Length);
		}
	}

	public static InputActionData State(int action)
	{
		var state = default(InputActionData);
		if (Native.IsConnected && action >= 0)
		{
			Native.Api->InputState(action, &state);
		}
		return state;
	}

	/// <summary>A control's bits and value by its path, or -1 for a path that names nothing.</summary>
	public static int Control(string path, out Vector2 value)
	{
		value = Vector2.Zero;
		if (!Native.IsConnected)
		{
			return -1;
		}
		var bytes = Encoding.UTF8.GetBytes(path);
		var read = stackalloc float[2];
		int flags;
		fixed (byte* written = bytes)
		{
			flags = Native.Api->InputControl(written, bytes.Length, read);
		}
		value = new Vector2(read[0], read[1]);
		return flags;
	}

	/// <summary>The actions a control serves, a line each, as input_uses writes them; empty without the engine.</summary>
	public static string Uses(string path)
	{
		return Native.IsConnected ? ByPath(path, Native.Api->InputUses) : string.Empty;
	}

	/// <summary>The other actions bound to a control that can be on at once with an action, "Map/Name" a line; empty without the engine.</summary>
	public static string Conflicts(int action, string path)
	{
		if (!Native.IsConnected)
		{
			return string.Empty;
		}
		var bytes = Encoding.UTF8.GetBytes(path);
		fixed (byte* named = bytes)
		{
			var length = Native.Api->InputConflicts(action, named, bytes.Length, null, 0);
			var buffer = new byte[Math.Max(length, 0)];
			fixed (byte* into = buffer)
			{
				Native.Api->InputConflicts(action, named, bytes.Length, into, buffer.Length);
			}
			return Encoding.UTF8.GetString(buffer);
		}
	}

	/// <summary>A control as prompts name it; its path without the engine.</summary>
	public static string Label(string path)
	{
		return Native.IsConnected ? ByPath(path, Native.Api->InputLabel) : path;
	}

	/// <summary>The text a native call writes about a control, asking again with room for all of it.</summary>
	private static string ByPath(string path, delegate* unmanaged[Cdecl]<byte*, int, byte*, int, int> call)
	{
		var bytes = Encoding.UTF8.GetBytes(path);
		fixed (byte* named = bytes)
		{
			var length = call(named, bytes.Length, null, 0);
			var buffer = new byte[Math.Max(length, 0)];
			fixed (byte* into = buffer)
			{
				call(named, bytes.Length, into, buffer.Length);
			}
			return Encoding.UTF8.GetString(buffer);
		}
	}

	/// <summary>Text about input, or null when there is none.</summary>
	public static string? Text(int what, int action = 0)
	{
		if (!Native.IsConnected)
		{
			return null;
		}
		var length = Native.Api->InputText(what, action, null, 0);
		if (length < 0)
		{
			return null;
		}
		var buffer = new byte[length];
		fixed (byte* into = buffer)
		{
			Native.Api->InputText(what, action, into, buffer.Length);
		}
		return Encoding.UTF8.GetString(buffer);
	}

	public static int Change(int what, int action = 0, int binding = 0, int extra = 0, string text = "")
	{
		if (!Native.IsConnected)
		{
			return -2;
		}
		var bytes = Encoding.UTF8.GetBytes(text);
		fixed (byte* written = bytes)
		{
			return Native.Api->InputChange(what, action, binding, extra, written, bytes.Length);
		}
	}
}
