---
order: 1
---

This page puts a score and a button on screen. It takes three files: the **markup**, which lists what is on screen; a **style sheet**, which says how it looks; and a **script**, which makes the button work. A **Screen UI** puts the markup on screen.

## The menu's markup

Make a `UI` folder in your project, and in it a text file called `Hud.vui`:

```xml
<vui>
    <Style src="Hud.vss" />
    <Panel name="hud" class="hud">
        <Label name="score">Score: 0</Label>
        <Button name="add">Add a point</Button>
    </Panel>
</vui>
```

A `Panel` is a box that holds other things, a `Label` shows text, and a `Button` can be clicked. The `name` is how your script finds each one. See [[UI Markup]].

## The menu's style sheet

Beside it, make `Hud.vss`:

```css
.hud {
    position: absolute;
    left: 20px;
    top: 20px;
    gap: 8px;
}

#score {
    font-size: 24px;
    color: white;
}
```

`.hud` styles everything with the class `hud`, and `#score` the element named `score`. This puts the panel 20 pixels in from the top left of the screen, with a gap between its parts. See [[UI Style Sheets]].

## Showing the menu

In the Hierarchy, pick **+ ▸ Screen UI**, or add one to any Obj with **Add Component ▸ UI ▸ Screen UI**. Drag `Hud.vui` from the Project panel onto its **Document**. Click **Play**, and the score and button show over the Game view.

| Setting | What it does |
| --- | --- |
| **Document** | The `.vui` file it shows |
| **Order** | Which Screen UI shows over which: higher over lower |

## The script that makes the button work

```csharp
using Voxear;

namespace MyGame;

public sealed class ScoreHud : Behaviour
{
	private UiElement? score;
	private int points;

	protected override void OnUiBuilt(UiDocument ui)
	{
		score = ui.Find("score");
		ui.Find("add")!.Clicked += AddPoint;
	}

	private void AddPoint()
	{
		points++;
		score!.Text = $"Score: {points}";
	}
}
```

Add the behaviour to the Screen UI's Obj. `OnUiBuilt` runs once the document is built, after `Start`, and hands you the document. Find its elements there and subscribe to their events once. There's no need to check every frame. Click the button, and the score goes up.

`Obj.ScreenUi.Document` is the same document, ready from `Awake` on, for a behaviour that needs it sooner.

A script can also put a document on screen itself, with `UiDocument.Load`. See [[UI from Scripts]].

## Things to know about game UI

- UI shows only while the game runs. There is no UI while you edit, and stopping play takes it away.
- A Screen UI shows while its Obj is there. Switch the Obj off to hide it, keeping its handlers, and destroy it to close it.
- Clicks that land on your UI are taken by it; clicks anywhere else reach the game.
- Saving `Hud.vui` while the game plays shows the change at once. Elements are found again by their names, and keep their handlers.
- When your scripts reload while the game plays, `OnUiBuilt` runs again, so the button carries on. Handlers added in `Start` stop until you play again.
- Builds include every `.vui` file, and the style sheets it names, by themselves.
