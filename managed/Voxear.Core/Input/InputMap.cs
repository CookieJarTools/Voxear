using System;
using System.Collections.Generic;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// A map of actions, such as Player or UI, by its name, turned on and off together, as a game does for its menus. A map can
/// exclude others, which are never on while it is: turning it on turns them off, and a control they share with it is
/// not a conflict, as the Player and UI maps share the arrow keys.
/// <code>
/// var menus = Input.Map("UI");
/// menus.Enable(); // and Player, which UI excludes, is off
/// </code>
/// </summary>
public sealed record InputMap
{
	internal InputMap(string name)
	{
		Name = name;
	}

	public string Name { get; }

	/// <summary>Whether its actions read the devices, rather than nothing.</summary>
	public bool Enabled
	{
		get { return Facts().Enabled; }
		set { NativeInput.Change(NativeInput.ChangeMap, extra: value ? 1 : 0, text: Name); }
	}

	/// <summary>The maps that are never on while this one is.</summary>
	public IReadOnlyList<InputMap> Excludes
	{
		get
		{
			var found = new List<InputMap>();
			foreach (var name in Facts().Excludes)
			{
				found.Add(new InputMap(name));
			}
			return found;
		}
	}

	/// <summary>Turns its actions on, and the maps it excludes off, as when a menu opens.</summary>
	public void Enable()
	{
		Enabled = true;
	}

	/// <summary>Turns its actions off, so they read nothing.</summary>
	public void Disable()
	{
		Enabled = false;
	}

	public override string ToString()
	{
		return Name;
	}

	/// <summary>Its line of the maps' text: its name, 1 when it is on, and the maps it excludes, between tabs.</summary>
	private (bool Enabled, string[] Excludes) Facts()
	{
		foreach (var line in (NativeInput.Text(NativeInput.TextMaps) ?? string.Empty).Split('\n'))
		{
			var parts = line.Split('\t');
			if (parts[0] == Name)
			{
				var excludes = parts.Length > 2 ? parts[2].Split(',', StringSplitOptions.RemoveEmptyEntries) : Array.Empty<string>();
				return (parts.Length < 2 || parts[1] == "1", excludes);
			}
		}
		return (false, Array.Empty<string>());
	}
}
