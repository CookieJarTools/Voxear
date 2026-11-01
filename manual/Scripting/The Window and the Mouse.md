---
order: 10
---

A build profile sets how big the game's window is when it starts, and whether it fills the screen. Your scripts can read the window's size, change it while the game runs, and hide or hold the mouse pointer.

## The window's size

```csharp
Log.Info($"The window is {Screen.Width} by {Screen.Height}");

// A settings menu's choice:
Screen.SetResolution(1920, 1080, false);
Screen.Fullscreen = true;
```

`Screen.Width` and `Screen.Height` are in pixels. `SetResolution` asks for a window of that size, or the whole screen when its last value is `true`, and the window changes at the end of the frame.

In the editor, `Width` and `Height` are the Game view's, and `SetResolution` changes nothing. Try it with `vox run`, or in a build.

## The mouse pointer

```csharp
// A first-person game: the mouse turns the camera, with no pointer to see.
Cursor.Lock = CursorLock.Locked;

// Back to a menu:
Cursor.Lock = CursorLock.None;
Cursor.Visible = true;
```

| `Cursor.Lock` | The pointer |
| --- | --- |
| `CursorLock.None` | Moves freely, in and out of the window |
| `CursorLock.Locked` | Stays in place, hidden, so the mouse can turn a camera without limit |
| `CursorLock.Confined` | Stays inside the window, as a strategy game wants for scrolling at its edges |

`Cursor.Visible` shows or hides the pointer. A locked pointer is always hidden.

In the editor, the pointer stays free while the game plays, so you can still use the editor. Try locking it with `vox run`, or in a build.
