---
order: 11
---

Voxear draws the keyboard and mouse, or a gamepad, with every key your game binds lit up and a label beside the device naming its actions. You see this in the editor's **Input Actions** window while you set up your controls, and players can see it in your game, where they can also rebind their keys. Everything in it can be changed: its colours, which actions it shows, how it looks, and the screen around it.

## The Input Actions window

Open **Input Actions** from the Window menu or the command palette, or double-click your project's `.inputactions` file. It draws your actions as the game reads them, and redraws as soon as you save the file.

- **Keyboard** and **Gamepad** pick the device.
- The map menu shows one map's actions, such as only `Player`, or all of them.
- **Assigned** colours every bound key the same. **Categories** colours each key by its action's category (see below).
- **Hidden** labels hidden actions too.

The legend under the device says what each colour means. A key whose actions are all hidden shows dark and unlabelled, so you can see it is taken without it cluttering the picture.

## Categories and hidden actions

Group your actions into categories in the `.inputactions` file, each with a colour, and give each action its category:

```toml
[[category]]
name = "Movement"
color = "#3fb950"

[[category]]
name = "Combat"
color = "#a371f7"

[[map.action]]
name = "Jump"
category = "Movement"
bindings = ["Keyboard/Space", "Gamepad/South"]

[[map.action]]
name = "Noclip"
hidden = true
bindings = ["Keyboard/F9"]
```

`hidden = true` keeps an action out of the labels, which suits debug keys and anything players shouldn't rebind.

## A controls screen for players

`KeyBindingScreen` is a whole controls screen, ready to use. It covers the game, starts on the device the player used last, and has buttons for the keyboard and the gamepad, the same Categories and Hidden switches as the editor, a legend, and Close. A player clicks a key to see its actions, and clicks **Rebind** beside one to press a new key for it. Escape keeps the old one.

```csharp
namespace MyGame;

public sealed class PauseMenu : Behaviour
{
	protected override void Update()
	{
		if (Input.Pressed("Controls"))
		{
			var screen = KeyBindingScreen.Open("Controls");
			screen!.Rebound += (action, control) => Save.Write("controls", Input.SaveBindings());
		}
	}
}
```

Saving what the player changed is up to your game, as in the example: give the text back to `Input.LoadBindings` when the game starts. See [[Saving the Game]].

The screen is built from parts you can change rather than replace. `screen.Keymap` is the drawn device. `screen.Bar`, `screen.Legend` and `screen.Details` are the panels around it, and `screen.Document` holds them all, so you can add buttons, hide parts or restyle them. Its classes are `key-binding-screen` and the ones starting `kbs-`.

## The device on its own

`KeymapView` is the drawn device by itself, to put in your own screens:

```csharp
var controls = panel.Add(new KeymapView(ControlScheme.Gamepad) { Colors = KeymapColors.Categories });
controls.Map = "Player";
controls.KeyClicked += path => Log.Info($"{Input.ControlLabel(path)}: {Input.ControlUses(path).Count} actions");
```

In a UI document, write `<Keymap name="controls" />`, then find it with `KeymapView.Find(document, "controls")`. It follows rebinding by itself, every frame.

Style it in your sheets by its classes:

| Class | What it is |
| --- | --- |
| `keymap-key` | Each key, with `assigned`, `hidden` or `unused`, and `round`, `stick` or `pill` for its shape |
| `keymap-key-label` | The text on a key |
| `keymap-callout` | Each label round the device. Its border colour is the colour of its line |
| `keymap-callout-line` | Each action in a label |

A key's category colour is set on the key itself, so it wins over your sheets.

## Drawing it your own way

To draw controls a different way, such as a list, or icons in your game's own style, ask for the same facts the key map uses:

- `Input.ControlUses(path)` lists the actions a control serves: each action, which of its bindings, the part of a composite binding (such as `up` of WASD), its label ("Move (Up)"), its category, and whether it is hidden.
- `Input.ControlLabel(path)` names a control as the player's keyboard or pad does, so `Keyboard/Q` is "A" on a French keyboard, and `Gamepad/South` is "Cross" on a PlayStation pad.
- `Input.Categories` lists your categories with their colours.
- `Input.Rebind` waits for a new key for a binding (see [[Reading Input]]).
