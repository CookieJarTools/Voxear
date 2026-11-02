using System;
using System.Collections.Generic;
using System.Threading.Tasks;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// The game's input, as actions such as Jump and Move rather than keys, so players can rebind them and a gamepad
/// works the same as the keyboard. The actions come from the project's <c>.inputactions</c> file, or from Voxear's
/// own when it has none: Move, Look, Jump, Sprint, Crouch, Interact, Attack, Aim, Reload and Pause, and for menus,
/// Navigate, Submit and Cancel.
/// <code>
/// if (Input.Pressed("Jump")) Jump();
/// var move = Input.Vector2("Move");
/// </code>
/// For more, keep an <see cref="InputAction"/> from <see cref="Action"/>. Keyboard, Mouse and Gamepad read the
/// devices themselves.
/// </summary>
public static class Input
{
	private static readonly Dictionary<string, InputAction> found = new Dictionary<string, InputAction>();

	/// <summary>Fires when the player changes from the keyboard and mouse to a gamepad, or back, as prompts change with it.</summary>
	public static event Action<ControlScheme>? SchemeChanged;

	/// <summary>The scheme the player used last.</summary>
	public static ControlScheme Scheme
	{
		get { return NativeInput.Text(NativeInput.TextScheme) == "Gamepad" ? ControlScheme.Gamepad : ControlScheme.KeyboardMouse; }
	}

	/// <summary>The names of the maps the actions are in, such as Player and UI.</summary>
	public static IReadOnlyList<string> Maps
	{
		get
		{
			var text = NativeInput.Text(NativeInput.TextMaps);
			return string.IsNullOrEmpty(text) ? Array.Empty<string>() : text.Split('\n');
		}
	}

	/// <summary>Whether the action went down this frame.</summary>
	public static bool Pressed(string action)
	{
		return Action(action).Pressed;
	}

	/// <summary>Whether the action is held down.</summary>
	public static bool Held(string action)
	{
		return Action(action).Held;
	}

	/// <summary>Whether the action came up this frame.</summary>
	public static bool Released(string action)
	{
		return Action(action).Released;
	}

	/// <summary>The action's value: a button's from 0 to 1, or a value's from -1 to 1.</summary>
	public static float Value(string action)
	{
		return Action(action).Value;
	}

	/// <summary>A vector2 action's value, such as Move's, each way from -1 to 1.</summary>
	public static System.Numerics.Vector2 Vector2(string action)
	{
		return Action(action).Vector;
	}

	/// <summary>What to press for the action, as a prompt shows it: see <see cref="InputAction.Prompt"/>.</summary>
	public static string Prompt(string action)
	{
		return Action(action).Prompt;
	}

	/// <summary>
	/// An action by its name, such as "Jump", or with its map's, "Player/Jump", when two maps have one of that name.
	/// One the game does not have reads nothing, and says so once.
	/// </summary>
	public static InputAction Action(string name)
	{
		if (found.TryGetValue(name, out var known))
		{
			return known;
		}
		var id = NativeInput.Action(name);
		var full = id >= 0 ? NativeInput.Text(NativeInput.TextName, id) ?? name : name;
		var slash = full.IndexOf('/');
		var (map, action) = slash >= 0 ? (full.Substring(0, slash), full.Substring(slash + 1)) : (string.Empty, full);
		if (id < 0 && Native.IsConnected)
		{
			Log.Warning($"The game has no input action called {name}; it reads nothing");
		}
		var made = new InputAction(id, action, map);
		found[name] = made;
		return made;
	}

	/// <summary>Turns a map's actions on, as when a menu closes.</summary>
	public static void EnableMap(string map)
	{
		NativeInput.Change(NativeInput.ChangeMap, extra: 1, text: map);
	}

	/// <summary>Turns a map's actions off, so they read nothing, as while a menu is open.</summary>
	public static void DisableMap(string map)
	{
		NativeInput.Change(NativeInput.ChangeMap, extra: 0, text: map);
	}

	/// <summary>
	/// Waits for the player to press a control, then binds it to the action in place of the binding at
	/// <paramref name="binding"/> (or one part of a composite, such as "up"), as a rebinding screen does. Only a
	/// control of <paramref name="scheme"/> counts, when given. Escape gives up. Finishes with the control's path,
	/// such as <c>Keyboard/J</c>, or null when given up. The action reads nothing meanwhile, so the key pressed does
	/// not also act.
	/// </summary>
	public static Task<string?> Rebind(InputAction action, int binding = 0, string? part = null, ControlScheme? scheme = null)
	{
		var which = scheme == null ? -1 : (int)scheme.Value;
		NativeInput.Change(NativeInput.ChangeListen, action.Id, binding, which, part ?? string.Empty);
		return InputEvents.AwaitRebind();
	}

	/// <summary>
	/// The actions a control serves, by its path, such as "Keyboard/W" or "Gamepad/LeftStick": every binding of every
	/// action that reads it, rebound or not, in the order of the actions. A stick counts its halves and its press too,
	/// as a key map draws them as one.
	/// </summary>
	public static IReadOnlyList<ControlUse> ControlUses(string path)
	{
		var uses = new List<ControlUse>();
		foreach (var line in NativeInput.Uses(path).Split('\n', StringSplitOptions.RemoveEmptyEntries))
		{
			var parts = line.Split('\t');
			if (parts.Length < 6)
			{
				continue;
			}
			var binding = int.TryParse(parts[1], out var at) ? at : 0;
			var part = parts[2].Length > 0 ? parts[2] : null;
			var category = parts[4].Length > 0 ? parts[4] : null;
			uses.Add(new ControlUse(Action(parts[0]), binding, part, parts[3], category, parts[5] == "1"));
		}
		return uses;
	}

	/// <summary>A control as prompts name it, by its path: "Keyboard/Q" is "A" on an AZERTY keyboard, and "Gamepad/South" is "Cross" on a PlayStation pad.</summary>
	public static string ControlLabel(string path)
	{
		return NativeInput.Label(path);
	}

	/// <summary>Every one of the game's actions, in every map, in the order the input actions list them.</summary>
	public static IReadOnlyList<InputAction> Actions
	{
		get
		{
			var actions = new List<InputAction>();
			foreach (var line in (NativeInput.Text(NativeInput.TextActions) ?? string.Empty).Split('\n', StringSplitOptions.RemoveEmptyEntries))
			{
				actions.Add(Action(line.Split('\t')[0]));
			}
			return actions;
		}
	}

	/// <summary>
	/// The other actions of <paramref name="action"/>'s map that a control is bound to, by its path, as a controls
	/// screen warns of when a player picks a key that is taken.
	/// </summary>
	public static IReadOnlyList<InputAction> Conflicts(InputAction action, string path)
	{
		var found = new List<InputAction>();
		foreach (var name in NativeInput.Conflicts(action.Id, path).Split('\n', StringSplitOptions.RemoveEmptyEntries))
		{
			found.Add(Action(action.Map.Length > 0 ? $"{action.Map}/{name}" : name));
		}
		return found;
	}

	/// <summary>The categories key maps colour actions by, from the input actions, in order.</summary>
	public static IReadOnlyList<InputCategory> Categories
	{
		get
		{
			var categories = new List<InputCategory>();
			var text = NativeInput.Text(NativeInput.TextCategories) ?? string.Empty;
			foreach (var line in text.Split('\n', StringSplitOptions.RemoveEmptyEntries))
			{
				var parts = line.Split('\t');
				var color = Color.White;
				try
				{
					color = parts.Length > 1 && parts[1].Length > 0 ? Color.Hex(parts[1]) : Color.White;
				}
				catch (FormatException)
				{
				}
				categories.Add(new InputCategory(parts[0], color));
			}
			return categories;
		}
	}

	/// <summary>Gives up a rebinding waiting for a control.</summary>
	public static void CancelRebind()
	{
		NativeInput.Change(NativeInput.ChangeCancel);
	}

	/// <summary>Every action's bindings as they are now, as text to keep with the player's settings, such as in a save.</summary>
	public static string SaveBindings()
	{
		return NativeInput.Text(NativeInput.TextSaved) ?? string.Empty;
	}

	/// <summary>Reads back what SaveBindings gave. Returns false when the text could not be read.</summary>
	public static bool LoadBindings(string saved)
	{
		return NativeInput.Change(NativeInput.ChangeLoad, text: saved) == 0;
	}

	/// <summary>Puts every binding back as the game's actions have it.</summary>
	public static void ResetBindings()
	{
		NativeInput.Change(NativeInput.ChangeReset);
	}

	internal static void RaiseSchemeChanged(ControlScheme scheme)
	{
		SchemeChanged?.Invoke(scheme);
	}
}
