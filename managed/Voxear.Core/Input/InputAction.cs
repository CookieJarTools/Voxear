using System;
using System.Collections.Generic;
using System.Numerics;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// One of the game's input actions, such as Jump or Move, as it is this frame, read from whichever of its bindings
/// the player pushes furthest. Find one with <c>Input.Action("Jump")</c>, and keep it: it reads the action anew
/// each time it is asked. Its Started, Performed and Canceled events fire once a frame, before the behaviours'
/// Update, for those who would rather be told than ask.
/// </summary>
public sealed class InputAction
{
	private readonly int id;

	private Action<InputAction>? started;
	private Action<InputAction>? performed;
	private Action<InputAction>? canceled;

	internal InputAction(int id, string name, string map)
	{
		this.id = id;
		Name = name;
		Map = map;
	}

	/// <summary>The action's name, such as Jump.</summary>
	public string Name { get; }

	/// <summary>The name of the map it is in, such as Player.</summary>
	public string Map { get; }

	/// <summary>Whether the game's actions have one by this name. One that does not reads nothing.</summary>
	public bool Exists => id >= 0;

	/// <summary>Whether it went down this frame.</summary>
	public bool Pressed => (Read().Flags & InputActionData.Pressed) != 0;

	/// <summary>Whether it is held down: a button pushed past halfway, or a value or vector off zero.</summary>
	public bool Held => (Read().Flags & InputActionData.Held) != 0;

	/// <summary>Whether it came up this frame.</summary>
	public bool Released => (Read().Flags & InputActionData.Released) != 0;

	/// <summary>
	/// Whether its interaction performed it this frame: when pressed, for most; once held long enough, for one
	/// with <c>hold</c>; and when let go quickly, for one with <c>tap</c>.
	/// </summary>
	public bool PerformedThisFrame => (Read().Flags & InputActionData.Performed) != 0;

	/// <summary>Its value: a button's from 0 to 1, or a value's from -1 to 1.</summary>
	public float Value => Read().X;

	/// <summary>Its value as a vector, for a vector2 action such as Move.</summary>
	public Vector2 Vector
	{
		get
		{
			var state = Read();
			return new Vector2(state.X, state.Y);
		}
	}

	/// <summary>How long it has been held, in seconds.</summary>
	public float HeldFor => Read().HeldFor;

	/// <summary>
	/// What to press for it in the scheme the player used last, as a prompt shows it: "Space", "W A S D", or a
	/// pad's "A" or "Cross". Empty when it has no binding there.
	/// </summary>
	public string Prompt => NativeInput.Text(NativeInput.TextPrompt, id) ?? string.Empty;

	/// <summary>
	/// Its bindings as they are now, one a line: a control's path, or a composite's parts, such as
	/// <c>up=Keyboard/W left=Keyboard/A down=Keyboard/S right=Keyboard/D</c>.
	/// </summary>
	public IReadOnlyList<string> Bindings
	{
		get
		{
			var text = NativeInput.Text(NativeInput.TextBindings, id);
			return string.IsNullOrEmpty(text) ? Array.Empty<string>() : text.Split('\n');
		}
	}

	/// <summary>Fires the frame it goes down.</summary>
	public event Action<InputAction>? Started
	{
		add { started += value; InputEvents.Watch(this); }
		remove { started -= value; }
	}

	/// <summary>Fires each frame its interaction performs it.</summary>
	public event Action<InputAction>? Performed
	{
		add { performed += value; InputEvents.Watch(this); }
		remove { performed -= value; }
	}

	/// <summary>Fires the frame it comes up.</summary>
	public event Action<InputAction>? Canceled
	{
		add { canceled += value; InputEvents.Watch(this); }
		remove { canceled -= value; }
	}

	/// <summary>Points one of its bindings at another control, such as <c>Keyboard/J</c>; for a composite, one part of it, such as "up".</summary>
	public bool Rebind(int binding, string control, string? part = null)
	{
		return NativeInput.Change(NativeInput.ChangeRebind, id, binding, PartIndex(part), control) == 0;
	}

	internal int Id => id;

	internal bool HasListeners => started != null || performed != null || canceled != null;

	internal void Raise()
	{
		var state = Read();
		if ((state.Flags & InputActionData.Pressed) != 0)
		{
			started?.Invoke(this);
		}
		if ((state.Flags & InputActionData.Performed) != 0)
		{
			performed?.Invoke(this);
		}
		if ((state.Flags & InputActionData.Released) != 0)
		{
			canceled?.Invoke(this);
		}
	}

	internal static int PartIndex(string? part)
	{
		return part switch
		{
			"up" => 0,
			"left" => 1,
			"down" => 2,
			"right" => 3,
			"negative" => 4,
			"positive" => 5,
			_ => -1,
		};
	}

	private InputActionData Read()
	{
		return NativeInput.State(id);
	}
}
