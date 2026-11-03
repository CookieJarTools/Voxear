using System.Numerics;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// One player of a game played by several on one machine, as Unity's PlayerInput is: the device they joined with,
/// and every action as they alone play it. Players join while Input.Joining is on, by pressing a button on a device
/// no one has yet.
/// <code>
/// var player = Input.Players[1];
/// if (player.Pressed("Jump"))
/// {
/// 	Jump(player.Index);
/// }
/// </code>
/// </summary>
public readonly struct InputPlayer
{
	internal InputPlayer(int index, bool connected, InputDevice device, string deviceName)
	{
		Index = index;
		Connected = connected;
		Device = device;
		DeviceName = deviceName;
	}

	/// <summary>Their number, from 0 for the first to join.</summary>
	public int Index { get; }

	/// <summary>Whether their device is there: the keyboard always is, a pad while it is plugged in.</summary>
	public bool Connected { get; }

	public InputDevice Device { get; }

	/// <summary>Their device as the system names it, such as "Xbox Wireless Controller".</summary>
	public string DeviceName { get; }

	/// <summary>Whether the action went down this frame, on their device.</summary>
	public bool Pressed(string action)
	{
		return (Read(action).Flags & InputActionData.Pressed) != 0;
	}

	public bool Held(string action)
	{
		return (Read(action).Flags & InputActionData.Held) != 0;
	}

	public bool Released(string action)
	{
		return (Read(action).Flags & InputActionData.Released) != 0;
	}

	public float Value(string action)
	{
		return Read(action).X;
	}

	public Vector2 Vector2(string action)
	{
		var state = Read(action);
		return new Vector2(state.X, state.Y);
	}

	/// <summary>Takes the player away; those after them move up a number.</summary>
	public void Remove()
	{
		NativeInput.Change(NativeInput.ChangeRemovePlayer, 0, Index);
	}

	private InputActionData Read(string action)
	{
		var id = NativeInput.Action(action);
		// The player's own reading of an action: one more than their number, above the action's.
		return id < 0 ? default : NativeInput.State(id | ((Index + 1) << 16));
	}
}
