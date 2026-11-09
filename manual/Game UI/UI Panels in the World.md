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

The layout never changes with distance, and a panel's text is sharp at any distance: it's drawn for each pixel on screen, from glyphs made once for every size. The rest of the panel, its backgrounds, borders and pictures, is drawn at the size the panel shows on screen, so it's crisp up close and costs less far away. Coming nearer, that sharpens straight away. Moving off, it waits a little before it drops detail, so it isn't drawn afresh at every step.

## Pointing at a panel

- With the cursor showing, the pointer works the panel it's over: buttons light up under it, and clicks and the wheel go to the panel.
- With the cursor locked, as in a first-person game, the middle of the screen points, where the crosshair is.
- A click that lands on a panel doesn't reach the game, so pressing a panel's button doesn't also fire a gun. A click anywhere else does.
- Something solid in front of a panel stops the pointer reaching it. Only colliders count.
- Typing goes to a panel while one of its text fields has focus. A click anywhere else gives the keys back to the game.

## Panels from scripts

`Obj.UiPanel.Document` is the document a panel shows, found and changed as a screen document is (see [[UI from Scripts]]). It's ready as soon as the panel's Obj is, so find its elements and subscribe to their events once, in `Start`:

```csharp
public sealed class Terminal : Behaviour
{
	protected override void Start()
	{
		var screen = Obj.UiPanel.Document!;
		var status = screen.Find("status")!;
		screen.Find("open")!.Clicked += () => status.Text = "Open";
	}
}
```

`Document` is null only when the panel has no document, or its file can't be read. The Console says why.

Scripts can also change a panel's `Size`, `PixelsPerMetre`, `FaceCamera`, `TwoSided` and `Clickable`, give an Obj a panel with `Obj.AddUiPanel()`, and take it off with `Remove()`.

## Subscribing in OnUiBuilt

A behaviour on the panel's Obj can subscribe in `OnUiBuilt` instead, which is handed the document. It runs after `Start`, before the first `Update`, and again each time the panel builds a new document:

```csharp
protected override void OnUiBuilt(UiDocument ui)
{
	var status = ui.Find("status")!;
	ui.Find("open")!.Clicked += () => status.Text = "Open";
}
```

How long handlers last:

| When | Added in Start | Added in OnUiBuilt |
| --- | --- | --- |
| The Obj is switched off and on | Kept | Kept |
| The document's file is saved | Kept | Kept |
| The panel is given another document | Stay with the old one | Added again |
| Scripts reload while the game plays | Lost | Added again |

When the file is saved, each element is found again by its name, so one you rename loses its handlers. Elements your scripts added to the document stay.

## Things to know about UI Panels

- Panels show while you edit, as well as while the game plays. Their buttons work only while it plays.
- A panel is drawn over the finished picture, so its colours are exactly as designed: light, fog and the camera's exposure don't change them. Smoke and particles don't cover it either.
- Seen at a slant, a panel's text stays sharp, so it reads from well off to one side. Its backgrounds and borders stay sharp along the way the panel isn't squeezed, and soften only nearly edge on.
- Saving a panel's document, or a style sheet it names, shows the change on the panel.
- A panel on an Obj that's switched off doesn't show, and the pointer passes through where it was.
