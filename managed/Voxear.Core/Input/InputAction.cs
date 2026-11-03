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
		Map = new InputMap(map);
	}

	/// <summary>The action's name, such as Jump.</summary>
	public string Name { get; }

	/// <summary>The map it is in, such as Player.</summary>
	public InputMap Map { get; }

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
	/// Its bindings as they are now, one a line, as the engine writes them: a control's path, or a composite's parts,
	/// such as <c>up=Keyboard/W left=Keyboard/A</c>. GetBindings reads them.
	/// </summary>
	internal IReadOnlyList<string> Bindings
	{
		get
		{
			var text = NativeInput.Text(NativeInput.TextBindings, id);
			return string.IsNullOrEmpty(text) ? Array.Empty<string>() : text.Split('\n');
		}
	}

	/// <summary>Its category, from the input actions, or null.</summary>
	public string? Category => Facts().Category;

	/// <summary>Whether the input actions mark it hidden, as a debug key: controls screens leave it out.</summary>
	public bool Hidden => Facts().Hidden;

	/// <summary>The name of the icon the input actions give it, one of Voxear's, or null for none.</summary>
	public string? Icon => Facts().Icon;

	/// <summary>
	/// Its bindings, each with its controls and what to call them, as a controls screen lists them; only those of
	/// <paramref name="scheme"/> when given. Each knows its place among all the action's bindings, for Rebind.
	/// </summary>
	public IReadOnlyList<InputBinding> GetBindings(ControlScheme? scheme = null)
	{
		var found = new List<InputBinding>();
		var lines = Bindings;
		for (var at = 0; at < lines.Count; at++)
		{
			var binding = InputBinding.Parse(this, at, lines[at]);
			if (scheme == null || binding.Scheme == scheme)
			{
				found.Add(binding);
			}
		}
		return found;
	}

	/// <summary>Puts its bindings back as the input actions have them, undoing the player's rebinding of it.</summary>
	public void ResetBindings()
	{
		NativeInput.Change(NativeInput.ChangeResetAction, id);
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

	/// <summary>
	/// Points one of its bindings at another control; for a composite, one part of it.
	/// <code>
	/// jump.Rebind(0, Key.J);
	/// move.Rebind(0, Key.Z, BindingPart.Up);
	/// </code>
	/// </summary>
	public bool Rebind(int binding, InputControl control, BindingPart? part = null)
	{
		return NativeInput.Change(NativeInput.ChangeRebind, id, binding, PartIndex(part), control.Path) == 0;
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

	internal static int PartIndex(BindingPart? part)
	{
		return part switch
		{
			BindingPart.Up => 0,
			BindingPart.Left => 1,
			BindingPart.Down => 2,
			BindingPart.Right => 3,
			BindingPart.Negative => 4,
			BindingPart.Positive => 5,
			_ => -1,
		};
	}

	private InputActionData Read()
	{
		return NativeInput.State(id);
	}

	private (string? Category, bool Hidden, string? Icon) Facts()
	{
		foreach (var line in (NativeInput.Text(NativeInput.TextActions) ?? string.Empty).Split('\n'))
		{
			var parts = line.Split('\t');
			if (parts.Length >= 3 && parts[0] == $"{Map}/{Name}")
			{
				var icon = parts.Length > 3 && parts[3].Length > 0 ? parts[3] : null;
				return (parts[1].Length > 0 ? parts[1] : null, parts[2] == "1", icon);
			}
		}
		return (null, false, null);
	}
}
