using System;
using System.Collections.Generic;
using System.Threading.Tasks;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// The game's input, as actions such as Jump and Move rather than keys, so players can rebind them and a gamepad
/// works the same as the keyboard. The actions come from the project's <c>.inputactions</c> file, or from Voxear's
/// own when it has none: Move, Look, Jump, Sprint, Crouch, Interact, Attack, Aim, Reload and Pause, and for menus,
/// Navigate, Submit and Cancel, in a UI map that is on only while turned on and turns Player off meanwhile.
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

	/// <summary>The maps the actions are in, such as Player and UI, in order.</summary>
	public static IReadOnlyList<InputMap> Maps
	{
		get
		{
			var maps = new List<InputMap>();
			foreach (var line in (NativeInput.Text(NativeInput.TextMaps) ?? string.Empty).Split('\n', StringSplitOptions.RemoveEmptyEntries))
			{
				maps.Add(new InputMap(line.Split('\t')[0]));
			}
			return maps;
		}
	}

	/// <summary>A map by its name, such as "UI", to turn on and off.</summary>
	public static InputMap Map(string name)
	{
		return new InputMap(name);
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

	/// <summary>Turns a map's actions on, and those of the maps it excludes off, as when a menu closes.</summary>
	public static void EnableMap(string map)
	{
		Map(map).Enable();
	}

	/// <summary>Turns a map's actions off, so they read nothing, as while a menu is open.</summary>
	public static void DisableMap(string map)
	{
		Map(map).Disable();
	}

	/// <summary>
	/// Waits for the player to press a control, then binds it to the action in place of the binding at
	/// <paramref name="binding"/> (or one part of a composite, such as Up), as a rebinding screen does. Only a control
	/// of <paramref name="scheme"/> counts, when given. Escape gives up. Finishes with the control, or null when given
	/// up. The action reads nothing meanwhile, so the key pressed does not also act.
	/// </summary>
	public static async Task<InputControl?> Rebind(InputAction action, int binding = 0, BindingPart? part = null, ControlScheme? scheme = null)
	{
		var which = scheme == null ? -1 : (int)scheme.Value;
		var named = part?.ToString().ToLowerInvariant() ?? string.Empty;
		NativeInput.Change(NativeInput.ChangeListen, action.Id, binding, which, named);
		return InputControl.FromPath(await InputEvents.AwaitRebind());
	}

	/// <summary>
	/// The actions a control serves: every binding of every action that reads it, rebound or not, in the order of the
	/// actions. A stick counts its halves and its press too, as a key map draws them as one.
	/// <code>
	/// var uses = Input.ControlUses(Key.Space);
	/// </code>
	/// </summary>
	public static IReadOnlyList<ControlUse> ControlUses(InputControl control)
	{
		var uses = new List<ControlUse>();
		foreach (var line in NativeInput.Uses(control.Path).Split('\n', StringSplitOptions.RemoveEmptyEntries))
		{
			var parts = line.Split('\t');
			if (parts.Length < 8)
			{
				continue;
			}
			var binding = int.TryParse(parts[1], out var at) ? at : 0;
			var category = parts[4].Length > 0 ? parts[4] : null;
			var icon = parts[6].Length > 0 ? parts[6] : null;
			uses.Add(new ControlUse(Action(parts[0]), binding, Part(parts[2]), parts[3], category, parts[5] == "1", icon, parts[7] == "1"));
		}
		return uses;
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
	/// The other actions a control is bound to that can be on at once with <paramref name="action"/>, as a controls
	/// screen warns of when a player picks a key that is taken: those of its own map, and of maps that do not exclude
	/// it. A stick conflicts with its halves, but not with its press.
	/// </summary>
	public static IReadOnlyList<InputAction> Conflicts(InputAction action, InputControl control)
	{
		var found = new List<InputAction>();
		foreach (var name in NativeInput.Conflicts(action.Id, control.Path).Split('\n', StringSplitOptions.RemoveEmptyEntries))
		{
			found.Add(Action(name));
		}
		return found;
	}

	/// <summary>A binding's part by the name the engine gives it, such as "up", or null for none.</summary>
	internal static BindingPart? Part(string name)
	{
		return Enum.TryParse<BindingPart>(name, ignoreCase: true, out var part) ? part : null;
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

	/// <summary>
	/// Whether a button pressed on a device no player has yet makes a new player of it, as a "press a button to join"
	/// screen does. Until anyone joins, the game reads every device as one, as a game for one player does.
	/// <code>
	/// Input.Joining = true;
	/// Input.PlayerJoined += player => Spawn(player.Index);
	/// </code>
	/// </summary>
	public static bool Joining
	{
		get { return PlayerLines()[^1] == "1"; }
		set { NativeInput.Change(NativeInput.ChangeJoining, 0, 0, value ? 1 : 0); }
	}

	/// <summary>The players who have joined, in the order they did.</summary>
	public static IReadOnlyList<InputPlayer> Players
	{
		get
		{
			var lines = PlayerLines();
			var players = new List<InputPlayer>();
			for (var at = 0; at < lines.Length - 1; at++)
			{
				var parts = lines[at].Split('\t');
				var device = parts.Length > 1 && parts[1] == "Gamepad" ? InputDevice.Gamepad : InputDevice.KeyboardMouse;
				players.Add(new InputPlayer(at, parts[0] == "1", device, parts.Length > 2 ? parts[2] : string.Empty));
			}
			return players;
		}
	}

	/// <summary>Runs with each player who joins, at the start of the frame after.</summary>
	public static event Action<InputPlayer>? PlayerJoined;

	/// <summary>The most players who may join; 4 unless set.</summary>
	public static void SetMostPlayers(int most)
	{
		NativeInput.Change(NativeInput.ChangeMostPlayers, 0, 0, most);
	}

	internal static void RaiseSchemeChanged(ControlScheme scheme)
	{
		SchemeChanged?.Invoke(scheme);
	}

	internal static void RaiseJoined()
	{
		if (PlayerJoined == null)
		{
			return;
		}
		var joined = NativeInput.Text(NativeInput.TextJoined);
		if (string.IsNullOrEmpty(joined))
		{
			return;
		}
		var players = Players;
		foreach (var line in joined.Split('\n'))
		{
			if (int.TryParse(line, out var at) && at < players.Count)
			{
				PlayerJoined(players[at]);
			}
		}
	}

	private static string[] PlayerLines()
	{
		return (NativeInput.Text(NativeInput.TextPlayers) ?? string.Empty).Split('\n');
	}
}
