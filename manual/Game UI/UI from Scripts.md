---
order: 4
---

There are two ways to make a game's UI: write it as markup in a `.vui` file and load it, or build it in code, element by element. You can mix them, adding elements in code to a document you loaded.

## Building UI in code

```csharp
var hud = UiDocument.Create()!;
var panel = hud.Root.Add(new Panel("hud"));
var score = panel.Add(new Label("0"));
var pause = panel.Add(new Button("Pause"));
pause.Clicked += Pause;
```

`UiDocument.Create()` puts an empty document on screen. `Root` is its top element, and `Add` puts an element at the end of another and gives it back, so you can keep it in a field and change it later. There is no need to find it by name.

The elements you can make are `Panel`, `ScrollView`, `Label`, `Button`, `Toggle`, `Slider`, `TextField`, `Foldout`, `Dropdown` and `Icon`:

```csharp
var volume = panel.Add(new Slider(0, 10, value: 4));
var sound = panel.Add(new Toggle("Sound", on: true));
var quality = panel.Add(new Dropdown("Low", "Medium", "High"));
var settings = panel.Add(new Foldout("Settings"));
settings.Add(new Label("Inside the foldout"));
```

An element made with `new` waits in your code until you add it to one on screen. You can set its text, classes, style and value first; they show when it is added. `IsShown` says whether it is on screen yet.

`Remove` takes an element out again, with everything in it, and `Clear` takes out everything added to an element. A removed element keeps its text and value, and can be added again anywhere. `Children` lists what was added to an element in code, and `Parent` the element it was added to.

Panels lay out in a column. Give them classes, as `new Panel("hud", "top")`, and style the classes in a sheet, or set a style straight on the element with `SetStyle`.

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

`Find` looks through a loaded document for the element whose `name` attribute is `resume`, as Unity's `Q("resume")` does, and gives the first one, or `null`. It is how code reaches elements that markup made. Finding the same name again gives the same element, so find each one once and keep it. Elements you build in code need no finding: keep what `Add` gives back.

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
| `Value` | A slider's value, or the option a dropdown has picked, by its place |
| `Checked` | Whether a toggle is on |
| `AddClass("open")`, `RemoveClass`, `ToggleClass`, `HasClass` | Changes its classes, and so its style |
| `SetStyle("width: 40%")` | Gives it its own style, replacing any it had from code or its `style` attribute |

Changing a class is the neat way to change how something looks: write the look in the style sheet, and switch the class from code. A health bar, for instance:

```csharp
healthBar!.SetStyle($"width: {health}%");
lowHealth!.Visible = health < 20;
```

## Reacting to changes

`Changed` runs your code when an element's value changes on screen: typed in, ticked, moved or picked. `WasChanged` says whether it changed since the last frame. The elements made in code also have their own events, which hand you the new value:

```csharp
volume.Moved += value => Audio.Volume = value;
sound.Toggled += on => Audio.Muted = !on;
quality.Picked += index => Settings.Quality = index;
name.Typed += text => player.Name = text;
```

Like `Clicked`, these run at the start of the next frame.

## Using UI with the keyboard

In a running game, Tab and Shift+Tab move between buttons, text fields, toggles, sliders and dropdowns. Enter or Space clicks a button or flips a toggle, the arrow keys move a slider or a dropdown's choice, and Escape lets go. The element you are on matches `:focus` in your style sheet, and `:focus-visible` too when you reached it with Tab, or when it is a text field. Style the focus ring with `:focus-visible`, so a button you click does not keep a ring as if marking what was pressed last.

The editor's Game view passes on less: the pointer, the left mouse button, and typing into a text field. To try keyboard control, run the game with `vox run`, or as a build.

Gamepads cannot drive UI yet.

## What C# cannot do with UI yet

Scripts cannot yet read which tab is picked, turn an element off with `:disabled`, show a picture in an `Image`, or fill a `ListView`. For a picked tab, name each `Tab`, whose page is `Visible` only while its tab is picked. Elements made in code cannot have a name, so `Find` does not find them; keep them instead.

## UI text and fonts

Text uses the fonts installed on the player's computer: Inter where it is there, or a similar one. Your own font files cannot be used yet. Labels hold plain text; bold or coloured words within one label are not possible yet, so use a label for each.
