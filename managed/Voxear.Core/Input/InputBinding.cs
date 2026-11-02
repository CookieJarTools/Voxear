using System;
using System.Collections.Generic;
using System.Linq;

namespace Voxear;

/// <summary>
/// One binding of an action, as a controls screen lists it: its place among the action's bindings, the scheme it is
/// for, and its controls, one for a plain binding or one a part for a composite such as WASD. Label names it as the
/// player's keyboard or pad does, as "Space" or "W A S D".
/// </summary>
public sealed class InputBinding
{
	private InputBinding(InputAction action, int index, ControlScheme scheme, IReadOnlyList<InputBindingControl> controls)
	{
		Action = action;
		Index = index;
		Scheme = scheme;
		Controls = controls;
	}

	public InputAction Action { get; }

	/// <summary>Its place among all the action's bindings, as Input.Rebind takes it.</summary>
	public int Index { get; }

	public ControlScheme Scheme { get; }

	public IReadOnlyList<InputBindingControl> Controls { get; }

	/// <summary>Whether it is made of parts, such as up, down, left and right.</summary>
	public bool IsComposite => Controls.Count > 1 || (Controls.Count == 1 && Controls[0].Part != null);

	public string Label => string.Join(" ", Controls.Select(control => control.Label));

	/// <summary>Reads a binding as InputAction.Bindings writes it: a path, or parts as <c>up=Keyboard/W left=Keyboard/A</c>.</summary>
	internal static InputBinding Parse(InputAction action, int index, string line)
	{
		var controls = new List<InputBindingControl>();
		foreach (var piece in line.Split(' ', StringSplitOptions.RemoveEmptyEntries))
		{
			var equals = piece.IndexOf('=');
			var part = equals > 0 ? piece.Substring(0, equals) : null;
			var path = equals > 0 ? piece.Substring(equals + 1) : piece;
			controls.Add(new InputBindingControl(part, path, Input.ControlLabel(path)));
		}
		var first = controls.Count > 0 ? controls[0].Path : string.Empty;
		var scheme = first.StartsWith("Gamepad/", StringComparison.Ordinal) ? ControlScheme.Gamepad : ControlScheme.KeyboardMouse;
		return new InputBinding(action, index, scheme, controls.AsReadOnly());
	}
}
