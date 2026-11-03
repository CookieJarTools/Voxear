---
order: 11
---

Voxear draws the keyboard and mouse, or a gamepad, with every key your game binds lit up and a label beside the device naming its actions. You see this in the editor's **Input Actions** window while you set up your controls. Players can see it in your game too, or a plain list of actions with their keys, as most games' settings show them, and rebind their keys from either. Everything in it can be changed: its colours, which actions it shows, how it looks, and the screen around it.

## The Input Actions window

Open **Input Actions** from the Window menu or the command palette, or double-click your project's `.inputactions` file. It has two views: **Actions**, where you make and change your actions, and **Layout**, which draws them on the devices.

### Actions

The Actions view is laid out as Unity's Input Actions window is, in three columns:

- **Action Maps**: the maps, such as `Player` and `UI`. **+** adds one, and **×** takes the picked map away.
- **Actions**: the picked map's actions, each followed by its bindings. A composite, such as WASD, shows each of its parts under it. **Action** adds an action, **Binding** adds a binding to the picked action, and **2D Vector** and **1D Axis** add a composite with its parts. **×** takes away the picked row.
- **Properties**: what the picked row has. An action has its **Name**, **Type** (Button, Value or Vector 2), **Interaction** (Press, Hold or Tap, with its **Seconds**), **Category** and whether it is **Hidden**. A binding, or a part of a composite, has its **Path**, such as `Keyboard/Space`, and its **Processors**, such as `deadzone(0.1, 0.9), invert`.

**Listen**, beside a binding's path, waits for the next key, mouse button or gamepad button you press, and binds it. Escape keeps the binding as it was.

Every change is saved to your project's `.inputactions` file straight away, and the game reads it again. If your project has no file yet, the first change makes `Input.inputactions`, starting from Voxear's own actions. A change the game couldn't read isn't kept, and the window says why.

### Layout

The Layout view draws your actions as the game reads them, and redraws as soon as the file changes.

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

`KeyBindingScreen` is a whole controls screen, ready to use. It covers the game and has two views:

- **Layout**, the device drawn with its actions. It starts on the device the player used last, with buttons for the keyboard and the gamepad, the same Categories switch as the editor, and a legend. A player clicks a key to see its actions, and clicks **Rebind** beside one to press a new key for it.
- **List**, a row for each action under its category, with its keys in one column and its gamepad buttons in another. A player clicks a key or button in a row, then presses the new one. A row shows when another action uses the same key, as "Also Interact", and **Reset** puts the row back.

Both have **Hidden**, to list hidden actions too, **Reset All**, and **Close**. Escape, while the screen waits for a new key, keeps the old one.

Pick the views when you open it: `ControlsView.Both` shows them as tabs, and `ControlsView.Layout` or `ControlsView.List` shows one alone.

```csharp
namespace MyGame;

public sealed class PauseMenu : Behaviour
{
	protected override void Update()
	{
		if (Input.Pressed("Controls"))
		{
			var screen = KeyBindingScreen.Open("Controls", ControlsView.Both);
			screen!.Rebound += (action, control) => Save.Write("controls", Input.SaveBindings());
		}
	}
}
```

Saving what the player changed is up to your game, as in the example: give the text back to `Input.LoadBindings` when the game starts. See [[Saving the Game]].

The screen is built from parts you can change rather than replace. `screen.Keymap` is the drawn device and `screen.List` the list. `screen.Bar`, `screen.Legend` and `screen.Details` are the panels around them, and `screen.Document` holds them all, so you can add buttons, hide parts or restyle them. Its classes are `key-binding-screen` and the ones starting `kbs-`.

## The list on its own

`ControlsList` is the list by itself, to put in your own settings menu, such as in a scroll view:

```csharp
var list = scroll.Add(new ControlsList { Map = "Player" });
list.Rebound += (action, control) => Save.Write("controls", Input.SaveBindings());
```

Its classes are `controls-list`, `controls-heading`, `controls-row`, `controls-action`, `controls-cell`, `controls-binding` (each key's button, with `listening` while it waits), `controls-none`, `controls-conflict` and `controls-reset`.

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

## Building your own

Both ready-made views are built from the same calls, so a screen of your own can do anything they do:

- `Input.Actions` lists every action, and each has its `Category` and whether it is `Hidden`.
- `action.GetBindings(scheme)` gives an action's bindings, each with its `Index`, its `Scheme`, and its `Controls`: each control's path, its part in a composite (such as `up` of WASD), and its `Label`.
- `Input.ControlUses(path)` lists the actions a control serves, as the drawn device labels them, such as "Move (Up)".
- `Input.ControlLabel(path)` names a control as the player's keyboard or pad does, so `Keyboard/Q` is "A" on a French keyboard, and `Gamepad/South` is "Cross" on a PlayStation pad.
- `Input.Conflicts(action, path)` lists the other actions of the same map bound to a control.
- `Input.Categories` lists your categories with their colours.
- `Input.Rebind(action, binding.Index, part, scheme)` waits for a new key for a binding (see [[Reading Input]]). `action.ResetBindings()` puts one action back, and `Input.ResetBindings()` every action.

> [!NOTE]
> Controls are named in text for now, such as "Space" or "A". Button icons for each kind of gamepad come in a later version of Voxear.
