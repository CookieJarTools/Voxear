---
order: 4
---

## Loading and closing a UI document

```csharp
var menu = UiDocument.Load("UI/Menu.vui");
```

`Load` reads a `.vui` file by its path in your project and puts it on screen, over the game and over any UI loaded before it. It gives back `null` if the file cannot be read; the Console says why. Load from a behaviour's `Start`, or a system's `OnCreate`: UI exists only while the game runs.

`menu.Close()` takes the document off screen, and `menu.IsOpen` says whether it is still there.

## Finding UI elements

```csharp
var resume = menu.Find("resume");
```

`Find` gives the first element in the document with that `name`, or `null`. Finding the same name again gives the same element, so find each one once and keep it.

## Reacting to clicks on UI

```csharp
resume!.Clicked += () => Resume();
```

`Clicked` runs your code when the element, or anything inside it, is clicked. It runs at the start of the next frame, before any `Update`. If you would rather check each frame, `WasClicked` says whether it was clicked since the last frame:

```csharp
protected override void Update()
{
	if (quit!.WasClicked)
	{
		Level.Load("Levels/Title.level");
	}
}
```

## Reading and changing UI elements

| Member | What it does |
| --- | --- |
| `Text` | A label's or button's text, or what is typed in a text field |
| `Visible` | Shows or hides the element. Hidden, it takes no space |
| `Value` | A slider's value |
| `Checked` | Whether a toggle is on |
| `AddClass("open")`, `RemoveClass`, `ToggleClass`, `HasClass` | Changes its classes, and so its style |
| `SetStyle("width: 40%")` | Gives it its own style, replacing any it had from code or its `style` attribute |

Changing a class is the neat way to change how something looks: write the look in the style sheet, and switch the class from code. A health bar, for instance:

```csharp
healthBar!.SetStyle($"width: {health}%");
lowHealth!.Visible = health < 20;
```

Sliders, toggles and text fields have no event for a change. Read their `Value`, `Checked` or `Text` when you need them, or check them each frame.

## Using UI with the keyboard

In a running game, Tab and Shift+Tab move between buttons, text fields, toggles, sliders and dropdowns. Enter or Space clicks a button or flips a toggle, the arrow keys move a slider or a dropdown's choice, and Escape lets go. The element you are on matches `:focus` in your style sheet.

The editor's Game view passes on less: the pointer, the left mouse button, and typing into a text field. To try keyboard control, run the game with `vox run`, or as a build.

Gamepads cannot drive UI yet.

## What C# cannot do with UI yet

Scripts cannot yet build elements in code, read which option a dropdown or tab has picked, turn an element off with `:disabled`, show a picture in an `Image`, or fill a `ListView`. For a picked option, name each `Segment` and read its `Checked`, or name each `Tab`, whose page is `Visible` only while its tab is picked.

## UI text and fonts

Text uses the fonts installed on the player's computer: Inter where it is there, or a similar one. Your own font files cannot be used yet. Labels hold plain text; bold or coloured words within one label are not possible yet, so use a label for each.
