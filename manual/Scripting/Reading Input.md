---
order: 10
---

Your scripts read the keyboard, the mouse and gamepads through **actions**: Jump, Move, Attack and so on, rather than keys. A player can then rebind them, and a gamepad works the same as the keyboard, with no extra code. It is Unity's Input System, ready to use.

## Reading an action

```csharp
using System.Numerics;
using Voxear;

namespace MyGame;

public sealed class Player : Behaviour
{
	protected override void Update()
	{
		var move = Input.Vector2("Move");
		Obj.Position += new Vector3(move.X, 0f, move.Y) * 5f * Time.Delta;

		if (Input.Pressed("Jump"))
		{
			Log.Info("Jump!");
		}
	}
}
```

| Call | What it says |
| --- | --- |
| `Input.Pressed("Jump")` | The action went down this frame |
| `Input.Held("Sprint")` | It is held down |
| `Input.Released("Aim")` | It came up this frame |
| `Input.Value("Throttle")` | Its value: a button's from 0 to 1, or a value's from -1 to 1 |
| `Input.Vector2("Move")` | A vector action's value, each way from -1 to 1 |

Move reads W, A, S and D, the arrow keys and the left stick, whichever is pushed furthest. Diagonals are as long as straight lines, so the player is no faster going that way. A stick has a small dead zone, so it rests at zero.

## The actions every game starts with

A project without actions of its own uses Voxear's:

| Map | Action | Keyboard and mouse | Gamepad |
| --- | --- | --- | --- |
| Player | Move | W A S D, arrow keys | Left stick |
| | Look | Mouse | Right stick |
| | Jump | Space | South (A, Cross) |
| | Sprint | Left Shift | Left stick press |
| | Crouch | C | East (B, Circle) |
| | Interact | E | North (Y, Triangle) |
| | Attack | Left click | Right trigger |
| | Aim | Right click | Left trigger |
| | Reload | R | West (X, Square) |
| | Pause | Escape | Start |
| UI | Navigate | Arrow keys | D-pad, left stick |
| | Submit | Enter | South |
| | Cancel | Escape | East |

Look reads the mouse in pixels moved this frame, and the stick from -1 to 1, so scale them apart in your camera code.

## Your own actions

Put a file ending in `.inputactions` in your project, and the game uses its actions in place of Voxear's. It is a text file like this:

```toml
format = 1

[[map]]
name = "Player"

[[map.action]]
name = "Move"
type = "vector2"
bindings = [
    { up = "Keyboard/W", down = "Keyboard/S", left = "Keyboard/A", right = "Keyboard/D" },
    "Gamepad/LeftStick",
]

[[map.action]]
name = "Jump"
bindings = ["Keyboard/Space", "Gamepad/South"]

[[map.action]]
name = "Charge"
interaction = "hold(0.6)"
bindings = ["Mouse/Right", "Gamepad/RightShoulder"]
```

- An action's `type` is `button` (the default), `value` or `vector2`.
- A binding is a control, such as `Keyboard/Space`, or several that make one: `up`, `down`, `left` and `right` make a vector, and `negative` and `positive` a value from -1 to 1.
- A binding's `processors` change what it reads: `deadzone(0.2, 0.9)`, `invert`, `invert_x`, `invert_y` and `scale(2)`, as in `{ path = "Gamepad/RightStick", processors = ["invert_y"] }`.
- An action's `interaction` says when it is performed: `hold(seconds)` once it has been held that long, or `tap(seconds)` when it is let go within that long.

Controls are named by device:

- **Keyboard**: a key by where it sits on a US keyboard, such as `Keyboard/W`, `Keyboard/1`, `Keyboard/Space`, `Keyboard/ShiftLeft` or `Keyboard/ArrowUp`. On an AZERTY keyboard, `Keyboard/W` is still the key above S, so WASD keeps its shape.
- **Mouse**: `Mouse/Left`, `Mouse/Right`, `Mouse/Middle`, `Mouse/Back`, `Mouse/Forward`, `Mouse/Delta` and `Mouse/Scroll`.
- **Gamepad**: the face buttons by where they sit, `Gamepad/South`, `East`, `West` and `North`, so South is A on an Xbox pad and Cross on a PlayStation one; `LeftShoulder`, `RightShoulder`, `LeftTrigger`, `RightTrigger`, `Select`, `Start`, `LeftStickPress`, `RightStickPress`, `DpadUp` and the other D-pad directions; `LeftStick` and `RightStick`, and their halves, such as `LeftStickUp`.

## Events

Keep an action, and listen to it:

```csharp
private InputAction fire = null!;

protected override void Start()
{
	fire = Input.Action("Attack");
	fire.Started += action => Log.Info("Fire!");
	fire.Canceled += action => Log.Info($"Held for {action.HeldFor:0.0} seconds");
}
```

`Started` fires the frame it goes down, `Performed` when its interaction performs it, and `Canceled` the frame it comes up. They fire before your behaviours' `Update`.

## Prompts

`Input.Prompt("Jump")` says what to press for an action, in the scheme the player used last: "Space" on the keyboard, or "A" on an Xbox pad and "Cross" on a PlayStation one. It changes as soon as the player picks up the other device. `Input.SchemeChanged` tells you when, to update your on-screen prompts.

## Rebinding

A settings screen lets players change their bindings. `Input.Rebind` waits for the next key or button pressed, and binds it:

```csharp
var jump = Input.Action("Jump");
var taken = await Input.Rebind(jump, binding: 0, scheme: ControlScheme.KeyboardMouse);
Log.Info(taken == null ? "Kept as it was" : $"Jump is now {jump.Prompt}");
```

Escape gives up. While it waits, the action reads nothing, so the key pressed does not also jump. To keep a player's bindings, save `Input.SaveBindings()` with their settings, and give it back to `Input.LoadBindings` when the game starts. `Input.ResetBindings()` puts every binding back.

## Menus

Turn a map off while a menu is open, so the player does not walk while choosing: `Input.DisableMap("Player")`, then `Input.EnableMap("Player")` when it closes.

## The devices themselves

For a key that is always that key, such as a debug key, read the device:

```csharp
if (Keyboard.WasPressed(Key.F3)) ShowStats();
if (Mouse.IsDown(MouseButton.Middle)) Pan(Mouse.Delta);
var throttle = Gamepad.RightTrigger;
```

## In the editor

The Game view hears the keyboard and mouse while the game plays, as soon as you press Play, and again after you click in it. Click elsewhere in the editor and it stops, so your keys work the editor again. Gamepads always reach the game while it plays.
