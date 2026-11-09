---
order: 4
---

There are two ways to make a game's UI: write it as markup in a `.vui` file and load it, or build it in code, element by element. You can mix them, adding elements in code to a document you loaded.

## Building UI in code

```csharp
var hud = UiDocument.Create()!;
var panel = hud.Root.Add(new Panel("hud"));
var score = panel.Add(new Label("0"));
panel.Add(new Button("Pause", Pause));
```

`UiDocument.Create()` puts an empty document on screen. `Root` is its top element, and `Add` puts an element at the end of another and gives it back, so you can keep it in a field and change it later. There is no need to find it by name. `new Button("Pause", Pause)` gives the button the method to run when it is clicked, as it is made.

The elements you can make are `Panel`, `ScrollView`, `Label`, `Button`, `Toggle`, `Slider`, `TextField`, `Foldout`, `Dropdown` and `Icon`:

```csharp
var volume = panel.Add(new Slider(0, 1, value: 0.8f));
var subtitles = panel.Add(new Toggle("Subtitles", on: true));
var quality = panel.Add(new Dropdown("Low", "Medium", "High"));
var name = panel.Add(new TextField { Placeholder = "Your name" });
var settings = panel.Add(new Foldout("Settings"));
settings.Add(new Label("Inside the foldout") { Tooltip = "Shows when the pointer rests here" });
```

An element made with `new` waits in your code until you add it to one on screen. You can set its text, classes, style and value first; they show when it is added. `IsShown` says whether it is on screen yet.

`Remove` takes an element out again, with everything in it, and `Clear` takes out everything added to an element. A removed element keeps its text and value, and can be added again anywhere. `Children` lists what was added to an element in code, and `Parent` the element it was added to.

Panels lay out in a column. Give them classes, as `new Panel("hud", "top")`, and style the classes in a sheet, or style the element itself with its `Style` (see [Styling elements from code](#styling-elements-from-code)).

## Loading and closing a UI document

The simplest way to show a document is a **Screen UI** on an Obj, with no code at all: see [[Your First Menu]]. A script can also load one itself. Give it a field for the document, and pick the file in the Inspector:

```csharp
[SerializeField] private UiDocumentAsset menuDocument;

protected override void Start()
{
	var menu = UiDocument.Load(menuDocument);
}
```

The field keeps pointing at the file when it is renamed or moved. `UiDocument.Load("UI/Menu.vui")` loads one by its path instead. Either puts the document on screen, over the game and over any UI loaded before it, and gives back `null` if the file can't be read; the Console says why. Load from a behaviour's `Start`, or a system's `OnCreate`: UI exists only while the game runs.

`menu.Close()` takes the document off screen, and `menu.IsOpen` says whether it is still there. A document a script loads stays until it is closed, even when the level changes, so close it in `OnDestroy` when it belongs to one level. A Screen UI does that by itself.

## Finding UI elements

```csharp
var resume = menu.Find("resume");
```

`Find` looks through a loaded document for the element whose `name` attribute is `resume`, as Unity's `Q("resume")` does, and gives the first one, or `null`. It is how code reaches elements that markup made. Finding the same name again gives the same element, so find each one once and keep it. Elements you build in code need no finding: keep what `Add` gives back.

## Reacting to clicks on UI

```csharp
resume!.Clicked += Resume;
```

`Clicked` runs your code when the element, or anything inside it, is clicked. It runs at the start of the next frame, before any `Update`. Subscribe once, where you find the element, usually in `Start`: a document is ready as soon as `Load` returns, and a UI Panel's as soon as its Obj is. There's no need to wait for it in `Update`.

If you would rather check each frame, `WasClicked` says whether it was clicked since the last frame:

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
| `Tooltip` | What shows under the element once the pointer rests on it for half a second: text, or a key's text |
| `Placeholder` | What a text field shows, faded, while nothing is typed in it |
| `Visible` | Shows or hides the element. Hidden, it takes no space |
| `Value` | A slider's value, or the option a dropdown has picked, by its place |
| `Checked` | Whether a toggle is on |
| `AddClass("open")`, `RemoveClass`, `ToggleClass`, `HasClass` | Changes its classes, and so its style |
| `Style.Width = 40`, `SetStyles(…)` | Changes its own style, a property at a time: see below |

Changing a class is the neat way to change how something looks: write the look in the style sheet, and switch the class from code. A health bar, for instance:

```csharp
healthBar!.Style.Width = Length.Percent(health);
lowHealth!.Visible = health < 20;
```

## Styling elements from code

An element's `Style` sets one property at a time. Each wins over the style sheets and keeps the rest, the `style` attribute's too:

```csharp
bar.Style.Position = Position.Absolute;
bar.Style.Left = 20;
bar.Style.Width = Length.Percent(50);
bar.Style.BackgroundColor = Color.Hex("#8b93ff");
```

`SetStyles` sets several at once, by name, and gives the element back, so it fits inside `Add`:

```csharp
var hud = root.Add(new Panel().SetStyles(position: Position.Absolute, left: 20, top: 20, flexDirection: FlexDirection.Row, gap: 12));
```

A number is pixels, `Length.Percent(50)` is half the box the element is in, and `Length.Auto` leaves it to the layout. Set a property to `null` to hand it back to the sheets. Reading one gives what code set, or `null`.

| Kind | Properties |
| --- | --- |
| Layout | `Display`, `Position`, `Left`, `Top`, `Right`, `Bottom`, `Width`, `Height`, `MinWidth`, `MinHeight`, `MaxWidth`, `MaxHeight`, `Overflow` |
| Flex | `FlexDirection`, `FlexGrow`, `FlexShrink`, `FlexBasis`, `FlexWrap`, `AlignItems`, `AlignSelf`, `JustifyContent`, `Gap`, `RowGap`, `ColumnGap` |
| Spacing | `Margin`, `Padding`, and each side, as `MarginLeft` or `PaddingTop` |
| Look | `BackgroundColor`, `BorderWidth`, `BorderColor`, `BorderRadius`, `Opacity`, `ZIndex`, `PointerEvents` |
| Text | `Color`, `FontSize`, `FontWeight`, `FontFamily`, `TextAlign`, `LineHeight` |

For a property `Style` doesn't have yet, such as a shadow or a shader, `SetStyle("box-shadow: 0 4px 12px #00000080")` takes CSS text. It replaces the element's whole style, from code and its `style` attribute.

## Reacting to changes

`Changed` runs your code when an element's value changes on screen: typed in, ticked, moved or picked. `WasChanged` says whether it changed since the last frame. The elements made in code also have their own events, which hand you the new value:

```csharp
volume.Moved += value => Audio.MasterVolume = value;
subtitles.Toggled += on => Settings.Subtitles = on;
quality.Picked += index => Settings.Quality = index;
name.Typed += text => player.Name = text;
```

Like `Clicked`, these run at the start of the next frame. Each can also be given as the element is made, and gives the element back, so it chains inside `Add`:

```csharp
panel.Add(new Slider(0, 1, value: 0.8f).OnMoved(value => Audio.MasterVolume = value));
panel.Add(new Label().Bind(() => $"Volume {Audio.MasterVolume * 100:0}%"));
```

`Bind` gives the label a function that works its text out again every frame, so it always shows the volume as it is. [[Callbacks and Binding]] has every callback and `Bind`, and when each runs.

## Showing a list of items

A `ListView` shows a list of items, one to a row, and scrolls when they do not fit. Only the rows on screen exist, so a list of thousands costs no more than a list of twenty. Each row shows its item's text:

```csharp
var names = hud.Root.Add(new ListView<string>(players));
names.Style.Height = 240;
names.SelectionChanged += () => Log.Info($"{names.SelectedItem} picked");
```

To show more than text, say how to make a row once, with `MakeRow`, and how to show an item in it, with `BindRow`. A row is made once and then shows whichever item scrolls into it:

```csharp
var scores = hud.Root.Add(new ListView<Player>(team)
{
	MakeRow = row =>
	{
		row.Add(new Label());
		row.Add(new Label()).AddClass("score");
	},
	BindRow = (row, player) =>
	{
		row.Children[0].Text = player.Name;
		row.Children[1].Text = player.Score.ToString();
	},
});
```

After you change the items, as by adding one, call `Refresh()` so every row shows them again. `SelectedIndex` and `SelectedItem` say what is selected, and setting `SelectedIndex` selects a row; `-1` is nothing. `ItemHeight` sets how tall each row is, in pixels.

## Showing a tree of items

A `TreeView` shows items inside items, each opened and closed with its arrow. Give it the items at the top, and how to find what is in each:

```csharp
var folders = hud.Root.Add(new TreeView<Folder>(drive.Folders, folder => folder.Folders));
folders.SelectionChanged += () => Open(folders.SelectedItem);
```

`MakeRow`, `BindRow`, `Refresh()` and `ItemHeight` work as a list's do, and `SelectedItem` says, or sets, what is selected. An item stays open and selected across a `Refresh()`, as each item is known by itself, the way a dictionary knows its keys.

## Using UI with the keyboard

In a running game, Tab and Shift+Tab move between buttons, text fields, toggles, sliders and dropdowns. Enter or Space clicks a button or flips a toggle, the arrow keys move a slider or a dropdown's choice, and Escape lets go. The element you are on matches `:focus` in your style sheet, and `:focus-visible` too when you reached it with Tab, or when it is a text field. Style the focus ring with `:focus-visible`, so a button you click does not keep a ring as if marking what was pressed last.

The editor's Game view passes on less: the pointer, the left mouse button, and typing into a text field. To try keyboard control, run the game with `vox run`, or as a build.

Gamepads cannot drive UI yet.

## What C# cannot do with UI yet

Scripts cannot yet read which tab is picked, turn an element off with `:disabled`, show a picture in an `Image`, or fill a `ListView` or `TreeView` declared in markup; make those in code instead. For a picked tab, name each `Tab`, whose page is `Visible` only while its tab is picked. Elements made in code cannot have a name, so `Find` does not find them; keep them instead.

## UI text and fonts

Text uses the fonts installed on the player's computer: Inter where it is there, or a similar one. Your own font files cannot be used yet. Labels hold plain text; bold or coloured words within one label are not possible yet, so use a label for each.
