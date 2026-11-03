---
order: 5
---

A **callback** is code you give an element to run when something happens to it: a button clicked, a slider moved. **Binding** is the other way round: you give an element a function, and it shows what the function gives, worked out again every frame, so a label keeps up with a score without being told.

Between them, they cover most of what a menu's code does. There are four ways to hear from an element, and each suits a different case:

| Way | Looks like | Use it for |
| --- | --- | --- |
| A callback, as the element is made | `new Button("Done", Close)`, `.OnMoved(…)` | Elements you build in code: the handler sits beside the element it belongs to |
| An event | `play.Clicked += Play;` | Elements found in markup, or a handler you add and take away later |
| `WasClicked` and `WasChanged` | `if (quit.WasClicked)` in `Update` | Code that already runs each frame and checks |
| `Bind` | `.Bind(() => $"{Coins} coins")` | Text or a value that should follow something that changes elsewhere |

## A settings panel built in code

This behaviour builds a settings panel with two volume sliders, a subtitles toggle, a difficulty dropdown and two buttons. Each element is made, given its callback and bound to what it shows, all in one line inside `Add`:

```csharp
using Voxear;

namespace MyGame;

public sealed class SettingsMenu : Behaviour
{
	private bool subtitles = true;
	private int difficulty = 1;

	protected override void Start()
	{
		var menu = UiDocument.Create()!;
		var panel = menu.Root.Add(new Panel("settings"));

		panel.Add(new Label().Bind(() => $"Volume {Audio.MasterVolume * 100:0}%"));
		panel.Add(new Slider(0, 1).Bind(() => Audio.MasterVolume).OnMoved(value => Audio.MasterVolume = value));
		panel.Add(new Label().Bind(() => $"Music {Audio.MusicVolume * 100:0}%"));
		panel.Add(new Slider(0, 1).Bind(() => Audio.MusicVolume).OnMoved(value => Audio.MusicVolume = value));

		panel.Add(new Toggle("Subtitles", subtitles) { Tooltip = "Shows what is said" }.OnToggled(on => subtitles = on));
		panel.Add(new Dropdown("Easy", "Normal", "Hard").Bind(() => difficulty).OnPicked(picked => difficulty = picked));

		panel.Add(new Button("Reset", Reset));
		panel.Add(new Button("Done", menu.Close));
	}

	private static void Reset()
	{
		Audio.MasterVolume = 1;
		Audio.MusicVolume = 1;
	}
}
```

- `new Button("Reset", Reset)` gives the button its callback as it is made. `Reset` runs each time it is clicked.
- `OnMoved`, `OnToggled` and `OnPicked` give a slider, a toggle and a dropdown a callback that is handed the new value. Each gives the element back as its own type, a `Slider` from a slider, so they chain, and `Add` still gives you the element to keep.
- Each label is bound to a function that writes out a volume. When a slider moves, the label under it says so the next frame.
- Each slider is bound to its volume too. Click **Reset** and `Reset` sets the volumes; the sliders move back to full by themselves, as they follow `Audio.MasterVolume` and `Audio.MusicVolume`.

An object initializer, `{ Tooltip = … }`, goes straight after `new Toggle(…)`, before the chained calls.

## The settings panel in markup

The same panel can be written in a `.vui` file, `UI/Settings.vui`:

```xml
<vui>
    <Panel class="settings">
        <Label name="volume-text" />
        <Slider name="volume" min="0" max="1" />
        <Toggle name="subtitles" value="true" tooltip="Shows what is said">Subtitles</Toggle>
        <Button name="reset">Reset</Button>
        <Button name="done">Done</Button>
    </Panel>
</vui>
```

The script finds each element by its `name`, then listens to its events:

```csharp
using Voxear;

namespace MyGame;

public sealed class SettingsMenu : Behaviour
{
	private UiDocument? menu;
	private UiElement? done;
	private bool subtitles = true;

	protected override void Start()
	{
		menu = UiDocument.Load("UI/Settings.vui");
		if (menu == null)
		{
			return;
		}

		var volume = menu.Find("volume")!;
		volume.Value = Audio.MasterVolume;
		volume.Changed += () => Audio.MasterVolume = volume.Value;
		menu.Find("volume-text")!.Bind(() => $"Volume {Audio.MasterVolume * 100:0}%");

		var toggle = menu.Find("subtitles")!;
		toggle.Changed += () => subtitles = toggle.Checked;

		menu.Find("reset")!.Clicked += () =>
		{
			Audio.MasterVolume = 1;
			volume.Value = 1;
		};
		done = menu.Find("done");
	}

	protected override void Update()
	{
		if (done != null && done.WasClicked)
		{
			menu!.Close();
		}
	}
}
```

`Find` gives a `UiElement`, not a `Slider` or a `Toggle`, so a found element has `Clicked` and `Changed`, `OnClicked` and `OnChanged`, and a text `Bind`, but not `OnMoved` or a slider's `Bind`. Read its new value in `Changed` from `Value` or `Checked`, and set its value by hand, as **Reset** does here.

**Done** is checked with `WasClicked` in `Update`, to show the other way: either works.

## When callbacks and bindings run

In a game, at the start of each frame:

1. The callbacks and events of everything clicked or changed since the last frame run. For one element, its own event (`Toggled`, `Moved`, `Typed` or `Picked`) runs first, then `Changed`, then `Clicked`.
2. Every binding is worked out again, and what changed is shown.
3. The behaviours' `Update` runs, where `WasClicked` and `WasChanged` say what happened since the last frame.

In the editor, a drawer works its bindings out each time it draws, after its callbacks, `Draw` and `Refresh` ([[Custom Drawers]]); an editor window or a side bar item does each frame it shows, after `Update` ([[Editor Windows]]).

Setting `Text` by hand stops a text binding, and the element keeps the text you set. A value binding keeps going: set a bound slider's `Value` and the next frame puts it back where its function says.

## Reference

### Callbacks

| Member | On | What it does |
| --- | --- | --- |
| `new Button(text, Action clicked)` | `Button` | A button that runs `clicked` each time it is clicked. `text` is a string or a key |
| `OnClicked(Action)` | Every element | Runs it each time the element, or anything in it, is clicked. Gives the element back |
| `OnChanged(Action)` | Every element | Runs it each time the element's value changes on screen: typed, ticked, moved or picked |
| `OnToggled(Action<bool>)` | `Toggle` | Runs it with whether the toggle is on, each time it is ticked or unticked |
| `OnMoved(Action<float>)` | `Slider` | Runs it with the value, each time the slider is moved |
| `OnTyped(Action<string>)` | `TextField` | Runs it with the text, each time something is typed |
| `OnPicked(Action<int>)` | `Dropdown` | Runs it with the option picked, by its place, each time one is picked |

### Events

| Member | On | What it does |
| --- | --- | --- |
| `Clicked` | Every element | Raised each time it is clicked |
| `Changed` | Every element | Raised each time its value changes on screen |
| `Toggled` | `Toggle` | Raised with whether it is on |
| `Moved` | `Slider` | Raised with the value |
| `Typed` | `TextField` | Raised with the text |
| `Picked` | `Dropdown` | Raised with the option picked, by its place |
| `WasClicked` | Every element | Whether it was clicked since the last frame |
| `WasChanged` | Every element | Whether its value changed on screen since the last frame |

Each event is heard on its own: a toggle needs only `Toggled`, with no `Changed` beside it.

### Binding

| Member | On | What it does |
| --- | --- | --- |
| `Bind(Func<string>)` | Every element | Shows what the function gives as its text, worked out again each frame |
| `Bind(Func<LocalizedString>)` | Every element | Shows the text of whichever key the function gives |
| `Bind(LocalizedString)` | Every element | Shows a key's text in the game's language, following it ([[Localized UI Text]]) |
| `Bind(Localized)` | Every element | Shows a `Localized` text, following its language and values |
| `Localize(LocalizedString, values)` | Every element | Shows a key's text with values filled in, following the language |
| `Bind(Func<float>)` | `Slider` | Puts the slider where the function says, each frame |
| `Bind(Func<bool>)` | `Toggle` | Ticks the toggle or not as the function says, each frame |
| `Bind(Func<int>)` | `Dropdown` | Picks the option the function says, by its place, each frame |

Every `Bind`, `Localize` and `On…` gives the element back as its own type, so they chain inside `Add`.
