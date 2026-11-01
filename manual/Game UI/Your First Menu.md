---
order: 1
---

This page puts a score and a button on screen. It takes three files: the **markup**, which lists what is on screen; a **style sheet**, which says how it looks; and a **script**, which loads it and makes the button work.

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

## The script that shows the menu

```csharp
using Voxear;

namespace MyGame
{
	public sealed class ScoreHud : Behaviour
	{
		private UiElement? score;
		private int points;

		protected override void Start()
		{
			var hud = UiDocument.Load("UI/Hud.vui");
			if (hud == null)
			{
				return;
			}

			score = hud.Find("score");
			hud.Find("add")!.Clicked += AddPoint;
		}

		private void AddPoint()
		{
			points++;
			score!.Text = $"Score: {points}";
		}
	}
}
```

Put the behaviour on any Obj in your level, such as an empty one called UI, and click **Play**. The score and button show over the Game view. Click the button, and the score goes up.

`UiDocument.Load` takes the path of the `.vui` file in your project. If it cannot be read, it gives back `null` and the Console says why. See [[UI from Scripts]].

## Things to know about game UI

- UI shows only while the game runs. There is no UI while you edit, and stopping play takes it away.
- Load each document once, in `Start`. Every `Load` puts another copy on screen.
- Clicks that land on your UI are taken by it; clicks anywhere else reach the game.
- When your scripts reload while the game plays, the UI stays on screen, but its buttons stop calling your code until you play again.
- Builds include every `.vui` file, and the style sheets it names, by themselves.
