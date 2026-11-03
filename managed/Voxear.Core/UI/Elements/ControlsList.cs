using System;
using System.Collections.Generic;
using System.Linq;

namespace Voxear;

/// <summary>
/// The game's controls as a list, as most games' settings show them: a row for each action, under its category,
/// with a column for the keyboard and mouse and one for a gamepad. Each control is a button that rebinds it: click
/// one, then press the new key or button. A row says which other actions share a control, and Reset puts the row's
/// bindings back.
/// <code>
/// var list = scroll.Add(new ControlsList { Map = Input.Map("Player") });
/// list.Rebound += (action, control) => Save.Write("controls", Input.SaveBindings());
/// </code>
/// <para>
/// Its classes are <c>controls-list</c>, <c>controls-heading</c>, <c>controls-row</c>, <c>controls-action</c>,
/// <c>controls-cell</c>, <c>controls-binding</c> (each control's button, <c>listening</c> while it waits),
/// <c>controls-none</c>, <c>controls-conflict</c> and <c>controls-reset</c>. For a list of your own, Input.Actions,
/// InputAction.GetBindings, Input.Conflicts and Input.Rebind are what it is made of.
/// </para>
/// </summary>
public sealed class ControlsList : Panel
{
	private static readonly ControlScheme[] Columns = [ControlScheme.KeyboardMouse, ControlScheme.Gamepad];

	private bool showHidden;
	private InputMap? map;

	public ControlsList() : base("controls-list")
	{
		Refresh();
	}

	/// <summary>Runs with the action and the control it now reads, each time the player rebinds one.</summary>
	public event Action<InputAction, InputControl>? Rebound;

	/// <summary>Whether actions marked hidden, as debug keys, are listed too.</summary>
	public bool ShowHidden
	{
		get { return showHidden; }
		set
		{
			showHidden = value;
			Refresh();
		}
	}

	/// <summary>Only this map's actions; null for every map's.</summary>
	public InputMap? Map
	{
		get { return map; }
		set
		{
			map = value;
			Refresh();
		}
	}

	/// <summary>Lists the actions again, as their bindings are now.</summary>
	public void Refresh()
	{
		Clear();
		var header = Add(new Panel("controls-row", "controls-header"));
		header.Add(new Label("Action")).AddClass("controls-action");
		header.Add(new Label("Keyboard and Mouse")).AddClass("controls-cell");
		header.Add(new Label("Gamepad")).AddClass("controls-cell");
		var shown = Input.Actions.Where(action => (showHidden || !action.Hidden) && (map == null || action.Map == map)).ToList();
		foreach (var group in shown.GroupBy(action => action.Category ?? string.Empty).OrderBy(group => Order(group.Key)))
		{
			Add(new Label(group.Key.Length > 0 ? group.Key : "General")).AddClass("controls-heading");
			foreach (var action in group)
			{
				AddRow(action);
			}
		}
	}

	/// <summary>Categories in the order the input actions list them, those without one last.</summary>
	private static int Order(string category)
	{
		var categories = Input.Categories;
		for (var at = 0; at < categories.Count; at++)
		{
			if (categories[at].Name == category)
			{
				return at;
			}
		}
		return int.MaxValue;
	}

	private void AddRow(InputAction action)
	{
		var row = Add(new Panel("controls-row"));
		row.Add(new Label(action.Name)).AddClass("controls-action");
		var conflicts = new List<string>();
		foreach (var scheme in Columns)
		{
			var cell = row.Add(new Panel("controls-cell"));
			AddBindings(cell, action, scheme, conflicts);
		}
		var reset = row.Add(new Button("Reset"));
		reset.AddClass("controls-reset");
		reset.Clicked += () =>
		{
			action.ResetBindings();
			Refresh();
		};
		if (conflicts.Count > 0)
		{
			row.Add(new Label($"Also {string.Join(", ", conflicts.Distinct())}")).AddClass("controls-conflict");
		}
	}

	/// <summary>A button for each control of the action's bindings for a scheme, noting the other actions each is bound to.</summary>
	private void AddBindings(Panel cell, InputAction action, ControlScheme scheme, List<string> conflicts)
	{
		var bindings = action.GetBindings(scheme);
		if (bindings.Count == 0)
		{
			cell.Add(new Label("None")).AddClass("controls-none");
		}
		foreach (var binding in bindings)
		{
			foreach (var control in binding.Controls)
			{
				AddControl(cell, binding, control);
				AddConflicts(conflicts, action, control.Control);
			}
		}
	}

	private static void AddConflicts(List<string> conflicts, InputAction action, InputControl? control)
	{
		if (control != null)
		{
			conflicts.AddRange(Input.Conflicts(action, control.Value).Select(other => other.Name));
		}
	}

	private void AddControl(Panel cell, InputBinding binding, InputBindingControl control)
	{
		var text = control.Part == null ? control.Label : $"{control.Part} {control.Label}";
		var button = cell.Add(new Button(text));
		button.AddClass("controls-binding");
		button.Clicked += () => Listen(button, binding, control.Part);
	}

	private async void Listen(Button button, InputBinding binding, BindingPart? part)
	{
		button.Text = "Press a key";
		button.AddClass("listening");
		var control = await Input.Rebind(binding.Action, binding.Index, part, binding.Scheme);
		Refresh();
		if (control != null)
		{
			Rebound?.Invoke(binding.Action, control.Value);
		}
	}
}
