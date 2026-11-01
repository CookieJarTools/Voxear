---
order: 5
---

A game running on its own, outside the editor, has tools of its own built in. They are there in two cases:

- **`vox run`**, which runs your project in a window of its own, straight from its files:

  ```bash
  vox run MyGame
  ```

- **A development build**, made with a Development profile. See [[Making a Build]].

Release builds have none of these tools, so players never see them.

## Keys

| Key | Does |
| --- | --- |
| F1 | Shows or hides the stats |
| F2 | Colours everything by its level of detail |
| F3 | Colours the shadows by which part of the shadow map they come from |
| F4 | Shows how many lights reach each part of the screen, from blue for few to red for many |
| F5 | Shows which way each surface faces, as colours |
| The key left of 1 | Opens the console |
| F12 | Takes a RenderDoc capture (see below) |

Press F2 to F5 again to go back to the normal picture.

## The stats

F1 shows a panel with the frame rate, how long each frame takes on the processor and the graphics card, a graph of recent frames, how many things were drawn and skipped, and the camera's exposure. Click its title to fold it away. `vox run MyGame --stats` starts with it showing.

## Flying around

Hold the right mouse button to fly the game's camera, as in the editor's Scene view: W, A, S and D to move, Q and E to go down and up, Shift to go faster, and the mouse wheel to change speed.

## The console

Press the key to the left of 1 (the same key whatever your keyboard layout) to open the console. It shows the game's messages, and runs what you type.

| Type | Does |
| --- | --- |
| `help` | Lists everything the console can run |
| `clear` | Empties the messages shown |
| `quit` | Closes the game |

Up and Down go back through what you typed, Tab finishes a name, and Escape closes the console.

Your own commands run here too, by their method name, with values after it:

```csharp
[Command("Spawn Wave")]
private static string SpawnWave(int count, string kind)
{
	// ...
	return $"Spawned {count} {kind}";
}
```

```
› SpawnWave 3 Wolves
Spawned 3 Wolves
```

Values can be text, numbers, `true` or `false`, the name of an enum value, or a vector written as `1,2,3`. Put text with spaces in quotes. Whatever the method returns is printed.

### Settings you can change while the game runs

Mark a static field or property with `[ConsoleVariable]` to read and change it from the console:

```csharp
[ConsoleVariable("Players take no damage")]
public static bool God;
```

```
› God
God = False
› God true
God = True
```

## Talking to a game from a terminal

`vox console` sends one line to a running game, as if you had typed it in its console, and prints what it says back. Name the project the game runs from, or for a development build, its `Data` folder:

```bash
vox console --project MyGame "SpawnWave 3 Wolves"
vox console --player "MyGame/Builds/Linux Development/Data" "God true"
```

This suits scripts and automated tests. The game only listens to programs on the same computer.

## RenderDoc captures

[RenderDoc](https://renderdoc.org) is a free tool for looking inside a frame, every draw and every image, in more depth than the Frame Debugger. Install it, then run your game with it loaded:

```bash
vox run MyGame --renderdoc
```

Press F12 to capture the next frame. RenderDoc opens with the capture, which is also saved in your project's `.voxear/Captures` folder.
