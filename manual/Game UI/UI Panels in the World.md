---
order: 6
---

A **UI Panel** shows a UI document on a rectangle in the world: a terminal by a door, a map pinned to a board, a sign over a shop, a prompt over something to pick up. Players read it as part of the world and click it by pointing at it. Make one with the Hierarchy's **+ ▸ UI Panel**, or add one to any Obj with **Add Component ▸ UI ▸ UI Panel**, then pick its **Document**.

The document is an ordinary `.vui` file with its style sheets, written as for the screen (see [[UI Markup]] and [[UI Style Sheets]]).

## UI Panel settings

| Setting | What it does |
| --- | --- |
| **Document** | The `.vui` document it shows |
| **Size** | Width and height in metres, before the Obj's scale |
| **Pixels per Metre** | How many of the document's pixels fit in a metre |
| **Face Camera** | Turns it to face the camera, keeping it upright |
| **Two Sided** | Shows it from behind as well, reading the right way round there, as a sign painted on both sides does. Off, it shows only from the front |
| **Clickable** | Whether the pointer works it. Turn it off for a sign, so clicks go to the game |

The panel is centred on its Obj, and reads the right way round when you look the way the Obj faces: point the Obj's forward away from whoever reads it. Seen from behind, a panel is **Two Sided** unless you turn that off: it reads the right way round from there too, and its buttons work from either side. Turn it off for a panel on a wall or a screen in a machine, which has no back to see.

## How big a panel's document is laid out

**Size** times **Pixels per Metre** is the size the document is laid out at. A panel 1.6 m wide at 500 pixels a metre lays its document out 800 pixels wide, as a screen 800 pixels wide would, so a label 16 pixels high stands 3.2 cm tall in the world. Raise Pixels per Metre to fit more on a panel, or lower it for bigger text.

The layout never changes with distance, but how sharply the panel is drawn does: it's drawn at the size it shows on screen, so its text is crisp up close and it costs less far away. Coming nearer, it sharpens straight away. Moving off, it waits a little before it drops detail, so it isn't drawn afresh at every step.

## Pointing at a panel

- With the cursor showing, the pointer works the panel it's over: buttons light up under it, and clicks and the wheel go to the panel.
- With the cursor locked, as in a first-person game, the middle of the screen points, where the crosshair is.
- A click that lands on a panel doesn't reach the game, so pressing a panel's button doesn't also fire a gun. A click anywhere else does.
- Something solid in front of a panel stops the pointer reaching it. Only colliders count.
- Typing goes to a panel while one of its text fields has focus. A click anywhere else gives the keys back to the game.

## Panels from scripts

`Obj.UiPanel.Document` is the document a panel shows, found and changed as a screen document is (see [[UI from Scripts]]). It's null until the document has loaded, a frame after the panel appears:

```csharp
public sealed class Terminal : Behaviour
{
	private UiElement? status;

	protected override void Update()
	{
		if (status != null)
		{
			return;
		}
		var screen = Obj.UiPanel.Document;
		if (screen == null)
		{
			return;
		}
		status = screen.Find("status");
		screen.Find("open")!.Clicked += () => status!.Text = "Open";
	}
}
```

Scripts can also change a panel's `Size`, `PixelsPerMetre`, `FaceCamera`, `TwoSided` and `Clickable`, give an Obj a panel with `Obj.AddUiPanel()`, and take it off with `Remove()`.

## Things to know about UI Panels

- Panels show while you edit, as well as while the game plays. Their buttons work only while it plays.
- A panel is drawn over the finished picture, so its colours are exactly as designed: light, fog and the camera's exposure don't change them. Smoke and particles don't cover it either.
- Seen at a slant, a panel stays sharp along the way it isn't squeezed, so its text still reads from well off to one side. Only nearly edge on does its text soften.
- Saving a panel's document, or a style sheet it names, shows the change on the panel.
