---
order: 1
---

Every piece of text your game shows can come from a **key**, such as `menu.play`. A key has its text in each of your project's languages, and the game shows it in the language the player picked, as Unity's Localization package does.

## Your project's languages

List your languages in `voxear.toml`, by their codes. The first is the language you write the game in:

```toml
[localization]
languages = ["en", "fr", "pt-BR"]
```

A code is a language, such as `fr`, or a language and a country, such as `pt-BR` for Brazilian Portuguese. Without a `[localization]` table, your game is in English only.

## String tables

A **string table** is a `.strings` file anywhere in your project. It holds some of your keys, each with its text in every language. Make as many as you like, such as one for the menus and one for the story, so that two people can work on them at once:

```toml
format = 1

[[entry]]
key = "menu.play"
comment = "The first button on the main menu"
limit = 12
en = "Play"
fr = "Jouer"
pt-BR = "Jogar"

[[entry]]
key = "loot.found"
en = "{player} found {count} coins"
fr = "{player} a trouvé {count} pièces"
```

`comment` tells a translator where the text shows, and `limit` is the most characters there is room for. Both are optional. Every table goes into every build, so no table needs to be in a level.

Select a table in the Project panel to edit it in the Inspector. Each entry shows its key, a box for each of your languages and its comment, saved as you leave each box. A language with no text for the key says **Missing** in a yellow box, and text longer than the key's `limit` turns yellow. Drag an entry by its grip to move it, and use **Add Entry** for a new one.

The editor reads your tables again whenever one changes, from the Inspector or from another program.

## Text in UI documents

In a UI document, write a key with `@` in front of it in place of a label's, a button's or a toggle's text. The element shows that key's text, and changes by itself when the game's language changes:

```xml
<vui>
    <Label class="title" text="@menu.title" />
    <Button name="play">@menu.play</Button>
</vui>
```

A script does the same by setting an element's `Text` to a key with `@` in front: `playButton.Text = "@menu.play"`. Setting plain text stops the element following a key. To start a text with a plain `@`, write it twice: `@@home` shows as `@home`.

## Text from scripts

`Localization.Get` gives a key's text in the language the game is in:

```csharp
namespace MyGame;

public sealed class MainMenu : Behaviour
{
	protected override void Start()
	{
		Log.Info(Localization.Get("menu.play"));
	}
}
```

`Localization.Format` fills in the values a text names. `{player} found {count} coins` takes the values named `player` and `count`:

```csharp
var found = Localization.Format("loot.found", ("player", name), ("count", coins));
```

A language changes with `Localization.Language`, such as from a settings menu. `Localization.Languages` lists your project's languages, and `Localization.Changed` tells you when the language changed, so you can show your text again:

```csharp
Localization.Changed += () => title.Text = Localization.Get("menu.title");
Localization.Language = "fr";
```

## Text in a behaviour's fields

A `LocalizedString` field holds a key, picked in the Inspector from a list of your project's keys, each shown beside its text. Its `Value` is the key's text in the game's language:

```csharp
namespace MyGame;

public sealed class Sign : Behaviour
{
	public LocalizedString Message = "sign.welcome";

	protected override void Start()
	{
		Log.Info(Message.Value);
	}
}
```

The scene saves the key, so a sign placed in a level reads in whatever language the player picks. `Message.Format(("player", name))` fills in values, as `Localization.Format` does.

## Translating in a spreadsheet

Translators often work in a spreadsheet. `vox loc export` writes a string table as a CSV file, with a row for each key and a column for each language, in the order your project lists them:

```bash
vox loc export Text/Menu.strings
```

This writes `Text/Menu.csv` beside the table. Open it in any spreadsheet, such as LibreOffice Calc or Google Sheets, fill in the empty cells, and save it as CSV again. `vox loc import` then says what the sheet would change:

```bash
vox loc import Menu.csv Text/Menu.strings
```

It lists each key the sheet adds (`+`), changes (`~`, with the languages that changed) and takes away (`-`). When that is what you want, run it again with `--apply` to keep it. The sheet replaces the table, so a row deleted from the sheet takes that key away.

## Which language the game starts in

The game starts in the language the player's computer is set to, when your project has it. When it doesn't, the game takes the nearest language your project has, so a player in Portugal (`pt-PT`) gets `pt` when you have it. Otherwise the game is in your own language, the first you listed.

A key with no text in the game's language falls back the same way: `pt-BR`, then `pt`, then your own language. A text you haven't translated yet still shows, in the language you wrote it in.

> [!NOTE]
> On Windows the game starts in your own language for now. Reading Windows' language settings comes in a later version of Voxear.

## Text that won't fit

Translated text is often longer than English, and German can be a third longer. Before anything is translated, run **Pseudo-localise Text** from the command palette. Every text from your tables then shows accented, a third longer and in brackets, as `[Ƥļåý ŧĥé ĝåɱé !!!!]`, both in the editor and while you play. A label that cuts its text off, or a button that spills over, shows straight away. So does any text your game shows that isn't in a table, as it stays plain. Run the command again to go back.

## Missing text

A key that no table has shows as `⟦menu.play⟧` while you make the game, in the editor and in development builds, and the Console warns about it once. A shipped game shows the key itself instead.
