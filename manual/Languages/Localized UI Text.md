---
order: 2
---

**Localized UI text** is an element, made in code, that shows a key's text rather than text you wrote out: a button that says **Play** in English and **Jouer** in French. The element follows the game's language, and changes by itself when the player picks another, as `@menu.play` does in markup. Each key is named in code as a typed key, such as `Strings.Menu.Play`, so a key that is missing or renamed is an error as you build, not text missing in the game.

For a menu written in a `.vui` file, write `@menu.play` in the markup ([[Translating Your Game]]); it needs no code. Build the elements in code, with the keys on this page, when the UI is made by your scripts anyway: a drawer, an editor window, a list made row by row. Code also reaches what markup cannot: a dropdown's options, a text field's placeholder and a foldout's title can all be keys from code, but are plain text in markup.

## A string table for a menu

Start with a string table, `Text/Menu.strings`, holding the menu's keys ([[Translating Your Game]] says how tables work):

```toml
format = 1

[[entry]]
key = "menu.play"
en = "Play"
fr = "Jouer"

[[entry]]
key = "menu.fill"
en = "Fill"
fr = "Remplir"

[[entry]]
key = "menu.quit"
en = "Quit"
fr = "Quitter"

[[entry]]
key = "menu.name-hint"
en = "Your name"
fr = "Votre nom"

[[entry]]
key = "hud.coins"
en = "{count, plural, one {# coin} other {# coins}}"
fr = "{count, plural, one {# pièce} other {# pièces}}"

[[entry]]
key = "hints.fill"
en = "Fills your health for 10 coins"
fr = "Remplit votre santé pour 10 pièces"

[[entry]]
key = "difficulty.easy"
en = "Easy"
fr = "Facile"

[[entry]]
key = "difficulty.hard"
en = "Hard"
fr = "Difficile"
```

## Typed keys

Each time your scripts build, Voxear makes a class, `Strings`, from every `.strings` table in your project. It is in your scripts' own namespace, so they use it with no `using`. The key `menu.play` is `Strings.Menu.Play`, a `LocalizedString`:

- The dots make groups, one inside another: `hud.coins` is `Strings.Hud.Coins`.
- Each part is written in PascalCase, its words split on dashes, underscores and spaces: `menu.name-hint` is `Strings.Menu.NameHint`.
- Your editor shows each key's text, in your first language, as you hover over it or pick it from the list.
- A key you misspell, rename or delete is a compile error where it is used.
- A key whose name is also a group's, such as both `menu` and `menu.play`, gets a warning and no member. Rename one of them.

A key in quotes, `"menu.play"`, still works wherever a `LocalizedString` goes, but nothing checks it.

## A main menu built in code

This behaviour builds the menu from the keys above:

```csharp
using Voxear;

namespace MyGame;

public sealed class MainMenu : Behaviour
{
	public int Coins = 12;

	protected override void Start()
	{
		var menu = UiDocument.Create()!;
		var panel = menu.Root.Add(new Panel("menu"));

		panel.Add(new Button(Strings.Menu.Play, Play));
		panel.Add(new Label().Bind(() => Strings.Hud.Coins.Format(("count", Coins))));
		panel.Add(new Button(Strings.Menu.Fill, Fill) { Tooltip = Strings.Hints.Fill });
		panel.Add(new TextField { Placeholder = Strings.Menu.NameHint });
		panel.Add(new Dropdown(Strings.Difficulty.Easy, Strings.Difficulty.Hard));
		panel.Add(new Button(Strings.Menu.Quit, Quit));
	}

	private void Fill()
	{
		Coins -= 10;
	}

	private static void Play()
	{
		Level.Load("Levels/First.level");
	}

	private static void Quit()
	{
		Level.Load("Levels/Title.level");
	}
}
```

- `new Button(Strings.Menu.Play, Play)` is a button with a key's text, and the method it runs when clicked. `Label`, `Toggle` and `Foldout` take a key in place of their text too.
- The coin label is bound to a function, as `Coins` changes. Each frame it works out the text again, with the number filled in, in the language the game is in now: **12 coins**, then **2 coins** after **Fill**.
- `Tooltip` is the text that shows under an element once the pointer rests on it for half a second. It takes a key or plain text, so `Tooltip = "Fills your health"` works too.
- `Placeholder` is what a text field shows, faded, while nothing is typed in it. It also takes a key or plain text.
- A `Dropdown` given keys shows each one's text as an option. `Options` gives the options as they show now.

## Text with values that stay the same

When the values a text names do not change, `Localize` fills them in once, and the element still follows the language:

```csharp
panel.Add(new Label().Localize(Strings.Hud.Coins, ("count", 3)));
```

Call `Localize` again for other values. For values that change while the text shows, use `Bind`, as the coin label above does. A tooltip with values is a `UiText`, made from the key and its values:

```csharp
panel.Add(new Button(Strings.Menu.Fill, Fill) { Tooltip = new UiText(Strings.Hud.Coins, ("count", 10)) });
```

## The main menu in markup

The same menu in a `.vui` file writes each key with `@` in front, and `tooltip` takes one as well:

```xml
<vui>
    <Panel class="menu">
        <Button name="play">@menu.play</Button>
        <Label name="coins" />
        <Button name="fill" tooltip="@hints.fill">@menu.fill</Button>
        <TextField name="name" placeholder="Your name" />
        <Button name="quit">@menu.quit</Button>
    </Panel>
</vui>
```

The coin count has values that change, so the script binds that label, and hands each button its method:

```csharp
var menu = UiDocument.Load("UI/Menu.vui")!;
menu.Find("play")!.Clicked += Play;
menu.Find("fill")!.Clicked += Fill;
menu.Find("quit")!.Clicked += Quit;
menu.Find("coins")!.Bind(() => Strings.Hud.Coins.Format(("count", Coins)));
```

The placeholder is plain text here, and the dropdown is left out: in markup, a placeholder and a dropdown's options cannot be keys yet. Build those in code when they need translating.

## Reference

| Member | What it does |
| --- | --- |
| `Strings.Group.Key` | A typed key from your string tables, a `LocalizedString` |
| `new Label(LocalizedString)` | A label showing a key's text. `Button`, `Toggle` and `Foldout` take one the same way |
| `new Button(LocalizedString, Action)` | A button showing a key's text, running the method when clicked |
| `new Dropdown(params UiText[])` | A dropdown whose options are keys' texts, or written out |
| `Options` | A dropdown's options, as they show now, in the game's language |
| `Localize(LocalizedString, values)` | Shows a key's text with values filled in. Call it again for other values |
| `Bind(LocalizedString)` | Shows a key's text, following the language |
| `Bind(Func<LocalizedString>)` | Shows the text of whichever key the function gives, worked out again each frame |
| `Bind(Func<string>)` | Shows what the function gives, as `() => Strings.Hud.Coins.Format(("count", Coins))` |
| `Bind(Localized)` | Shows a `Localized` text, following its language and values |
| `Tooltip` | What shows under any element when the pointer rests on it, a `UiText` |
| `Placeholder` | What a text field shows while empty, a `UiText` |
| `Text` | The element's text. Setting it stops a key or a `Bind` |

### UiText

A `UiText` is text an element shows: written out, or a key's text, which follows the game's language. A string or a `LocalizedString` turns into one by itself, so `Tooltip = "Fills"` and `Tooltip = Strings.Hints.Fill` both work.

| Member | What it does |
| --- | --- |
| `new UiText(string text)` | Text written out |
| `new UiText(LocalizedString key, values)` | A key's text, with the values it names filled in |
| `Key` | The key it shows, or none for text written out |
| `IsLocalized` | Whether it is a key's text |
| `IsEmpty` | Whether it shows nothing |
| `Values` | The values the key's text names |
| `ToString()` | What it shows now, in the game's language |
