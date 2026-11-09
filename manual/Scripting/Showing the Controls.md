---
order: 11
---

Voxear draws the keyboard and mouse, or a gamepad, with every key your game binds lit up and a label beside the device naming its actions. You see this in the editor's **Input Actions** window while you set up your controls. Players can see it in your game too, or a plain list of actions with their keys, as most games' settings show them, and rebind their keys from either. Everything in it can be changed: its colours, which actions it shows, how it looks, and the screen around it.

## The Input Actions window

Open **Input Actions** from the Window menu or the command palette, or double-click your project's `.inputactions` file. It has two views: **Actions**, where you make and change your actions, and **Layout**, which draws them on the devices.

### Actions

The Actions view is laid out as Unity's Input Actions window is, in three columns:

- **Action Maps**: the maps, such as `Player` and `UI`. **+** adds one, and **×** takes the picked map away. With no action picked, Properties shows the map's **Name** and **Excludes**: the maps that are never on while it is (see below).
- **Actions**: the picked map's actions, each followed by its bindings. A composite, such as WASD, shows each of its parts under it. **+** opens a menu: **Action** adds an action, **Binding** adds a binding to the picked action, and **2D Vector** and **1D Axis** add a composite with its parts. Each action's row has a **+** of its own, which adds a binding or a composite to that action. A new action is called **New Action** and its name is ready to type over in its row: type its name and press Enter, or Escape to keep New Action. A binding or part not given a control yet shows **None**. **×** takes away the picked row.
- **Properties**: what the picked row has. An action has its **Name**, **Type** (Button, Value or Vector 2), **Interaction** (Press, Hold or Tap, with its **Seconds**), **Category**, **Icon** and whether it is **Hidden**. A binding, or a part of a composite, has its **Control** and its **Processors**, such as `deadzone(0.1, 0.9), invert`.

Each row shows an icon: an action's own, or a binding's device. A binding shows its control by name, such as **Left Shift**, with its device after it. Every other row is shaded, so a long list is easy to follow.

Click a binding's **Control** to pick its control from a list of every control, in folders by device, as Unity's binding picker does: **Keyboard** (Letters, Numbers, Function Keys, Modifiers and so on), **Mouse** and **Gamepad** (Buttons, D-pad and each stick). Type to search them all at once: "shift" finds both Shift keys, "south" the pad's A. Controls are never typed by hand, so a binding can't be misspelt. **Listen**, under it, waits for the next key, mouse button or gamepad button you press instead, and binds it. Escape keeps the binding as it was.

Click an action's **Icon** to pick one of Voxear's icons for it, searched the same way. It marks the action in the Actions list and beside its name round the device in the Layout view. **Default** gives it its type's icon.

### Conflicts

A control bound to two actions that can be on at once is a **conflict**: two actions of the same map, or of two maps that don't exclude each other. The Actions list marks each conflicting row in red with a warning sign, and an action is marked when any of its controls conflicts. The Layout view colours a conflicting key red, whichever map it shows, and adds **Conflict** to the legend.

Maps that are never on together don't conflict. Voxear's own `UI` map excludes `Player`, so Navigate and Move can share the arrow keys. Tick **Excludes** on a map for each map it should never be on with; either map excluding the other is enough. In the file, a map lists them by name:

```toml
[[map]]
name = "UI"
excludes = ["Player"]
```

Turning a map on turns off the maps it excludes, and of two that exclude each other, only the first in the file starts on.

Every change is saved to your project's `.inputactions` file straight away, and the game reads it again. If your project has no file yet, the first change makes `Input.inputactions`, starting from Voxear's own actions. A change the game couldn't read isn't kept, and the window says why.

### Layout

The Layout view draws your actions as the game reads them, and redraws as soon as the file changes.

- **Keyboard** and **Gamepad** pick the device.
- The map menu shows one map's actions, such as only `Player`, or all of them.
- **Assigned** colours every bound key the same. **Categories** colours each key by its action's category (see below).
- **Hidden** labels hidden actions too.

The device is drawn as itself behind its keys: the keyboard's case and the mouse, with its side buttons (**M4** and **M5**) on its left, or a gamepad's body and grips. The legend under the device says what each colour means. A key whose actions are all hidden shows dark and unlabelled, so you can see it is taken without it cluttering the picture. A key's name is made smaller to fit when it is long, as **Backspace**.

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
var list = scroll.Add(new ControlsList { Map = Input.Map("Player") });
list.Rebound += (action, control) => Save.Write("controls", Input.SaveBindings());
```

Its classes are `controls-list`, `controls-heading`, `controls-row`, `controls-action`, `controls-cell`, `controls-binding` (each key's button, with `listening` while it waits), `controls-none`, `controls-conflict` and `controls-reset`.

## The device on its own

`KeymapView` is the drawn device by itself, to put in your own screens:

```csharp
var controls = panel.Add(new KeymapView(ControlScheme.Gamepad) { Colors = KeymapColors.Categories });
controls.Map = Input.Map("Player");
controls.KeyClicked += control => Log.Info($"{control.Label}: {Input.ControlUses(control).Count} actions");
```

In a UI document, write `<Keymap name="controls" />`, then find it with `KeymapView.Find(document, "controls")`. It follows rebinding by itself, every frame.

Style it in your sheets by its classes:

| Class | What it is |
| --- | --- |
| `keymap-key` | Each key, with `assigned`, `hidden`, `unused` or `conflict`, and `round`, `stick` or `pill` for its shape |
| `keymap-key-label` | The text on a key |
| `keymap-body` | Each part of the device's outline behind its keys, over a slightly bigger `keymap-body-edge` |
| `keymap-callout` | Each label round the device, with `conflict` for a conflicting key's. Its border colour is the colour of its line |
| `keymap-callout-line` | Each action in a label |
| `keymap-callout-icon` | An action's icon beside its name, in a `keymap-callout-row` |

A key's category colour is set on the key itself, so it wins over your sheets.

## Prompts

A prompt shows an action's controls, as games show "press A to jump": a picture of each, for the device the player used last. An Xbox pad's face buttons are lettered A, B, X and Y in green, red, blue and yellow, and a PlayStation pad's show its cross, circle, square and triangle. A prompt follows the device the player used last, and rebinding, by itself.

The pictures are Voxear's own icons, for the keyboard and mouse, Xbox, PlayStation, Switch and Steam Deck pads, unless your project has its own (see [[Showing the Controls#Your own icons|Your own icons]]). Pick **Drawn** as the Prompt Icons on the Input page to have Voxear draw each control instead, as a **glyph** your sheets can style: a keycap for a key, the mouse with its button filled, pills for shoulders and triggers, rings for the sticks and arrows for the d-pad.

In a UI document:

```xml
<Panel class="hint">
    <Prompt action="Jump" />
    <Label text="Jump" />
</Panel>
```

From a script, `new PromptView("Jump")` makes one, and its `Action` changes the action it shows. A composite shows each of its parts, so `Move` bound to WASD shows W, A, S and D.

To put the controls inside a sentence, as "Press [E] to open", use a `LocalizedLabel` with the action as one of its values: see [[Localized UI Text]].

### Voxear's icons

Voxear's icons are **Input Prompts** by Kenney, free for anyone to use and ship, in any game, with nothing owed. If they help your game, you can thank their maker by paying what you like for the pack at [kenney-assets.itch.io/input-prompts](https://kenney-assets.itch.io/input-prompts), or donating at [kenney.nl/donate](https://kenney.nl/donate).

A key your player's keyboard types differently, as Q types A on a French one, is drawn with the letter it types, rather than shown with a picture of the wrong letter.

## Your own icons

Prompts can show your own pictures for controls in place of Voxear's: a set of **Prompt Icons**.

1. In the Project panel, pick **Create ▸ Prompt Icons**.
2. In the Inspector, open a device: Keyboard, Mouse, Xbox, PlayStation, Switch or Steam. Each control has a row. Pick its picture, or drag one onto it from the Project panel.
3. On the Input page, pick the set as **Prompt Icons**, in the bar at the top.

A pad's pictures come from the set for the kind of pad the player holds, and change when they pick up another. A control your set has no picture for shows Voxear's, so a set can start small.

For a sheet of many icons in one picture, give a control the part of it to show, in the file, as `rect = [left, top, width, height]` in the picture's pixels:

```toml
[xbox]
South = { texture = "UI/Prompts/Xbox.png", rect = [0, 0, 64, 64] }
```

A picture shows 32 pixels square. Style `.prompt-sprite` in your sheets to change that.

Style the glyphs in your sheets by their classes: `prompt` for the whole prompt, and `prompt-glyph` for each control, with `key`, `round`, `pill`, `square`, `stick` or `bare` for its shape. Each holds a `prompt-text` label or a `prompt-icon`.

The Actions view of the Input Actions window shows a binding's glyph as its **Prompt**, so you can see what players will.

## Building your own

Both ready-made views are built from the same calls, so a screen of your own can do anything they do:

- `Input.Actions` lists every action, and each has its `Map`, `Category`, `Icon` and whether it is `Hidden`.
- `action.GetBindings(scheme)` gives an action's bindings, each with its `Index`, its `Scheme`, and its `Controls`: each `InputControl`, its `BindingPart` in a composite (such as `Up` of WASD), and its `Label`.
- An `InputControl` is made from the devices' own enums, so it is never spelt out: `InputControl jump = Key.Space;`, `InputControl.Mouse(MouseButton.Left)`, `InputControl.Gamepad(GamepadAxis.LeftStick)`. Its `Label` names it as the player's keyboard or pad does, so `Key.Q` is "A" on a French keyboard, and `GamepadButton.South` is "Cross" on a PlayStation pad.
- `Input.ControlUses(control)` lists the actions a control serves, as the drawn device labels them, such as "Move (Up)", each with its icon and whether it conflicts.
- `Input.Conflicts(action, control)` lists the other actions bound to a control that can be on at once with it.
- `Input.Maps` lists the maps; each has its `Excludes`, and `Enable()` and `Disable()`.
- `Input.Categories` lists your categories with their colours.
- `Input.Rebind(action, binding.Index, part, scheme)` waits for a new key for a binding (see [[Reading Input]]). `action.ResetBindings()` puts one action back, and `Input.ResetBindings()` every action.
