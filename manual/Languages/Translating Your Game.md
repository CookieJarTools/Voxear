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

### The Localization Tables window

To see every key at once, open **Localization Tables** from the Window menu or the command palette. It lists every key of every table, a row each, with its table and its text in each of your languages. A dash marks text that is missing, and the line at the top counts the keys and the missing text.

- **Search** finds keys by their name, their table or any of their text.
- Pick a key to see where it is used: UI documents that bind to it (`@menu.play`), and scenes, prefabs, levels and scripts that name it in quotes, such as a `LocalizedString` field or `Localization.Get("menu.play")`.
- **Rename** gives the picked key a new name in its table and everywhere it is used, then opens the level again to show the change. Save the level first, and stop playing, as renaming changes the files on disk.

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

### Text that keeps itself up to date

Text you show for a while, such as a coin counter, should change when the player picks another language, or when you edit its table while the game runs. A `Localized` does that for you: its `Text` is always right, and `Changed` tells you the new text whenever it changes, including when you give it a new value with `Set`:

```csharp
var coins = new Localized("hud.coins", ("count", 0));
counter.Text = coins.Text;
coins.Changed += text => counter.Text = text;

coins.Set("count", 12); // "12 coins", or "12 pièces" in French
```

A `LocalizedString` field gives one with `Observe()`. Call `Dispose()` on it once the text is no longer shown.

### Plurals, choices, numbers, dates and money

Languages count differently. English has "1 coin" and "2 coins", but Polish has a third form for 2 to 4, and Arabic has six. A text can hold every form, and the game picks the right one for the number and the language, as ICU's MessageFormat does:

```toml
[[entry]]
key = "loot.found"
en = "{player} found {count, plural, =0 {no coins} one {# coin} other {# coins}}"
pl = "{player} znalazł {count, plural, one {# monetę} few {# monety} many {# monet} other {# monety}}"
```

- `{count, plural, …}` picks a form by the language's plural rules: `zero`, `one`, `two`, `few`, `many` and `other`. Each language uses only some of them, and `other` is used when nothing else fits. `=0` picks an exact number before the rules do. Inside a form, `#` is the number.
- `{gender, select, female {her} male {his} other {their}}` picks a form by the value itself.
- `{count, number}` writes a number as the language writes them: `1,234.5` in English, `1 234,5` in French.
- `{when, date}` writes a date as the language writes them: `3 Nov 2026` in British English, `Nov 3, 2026` in American English. Add `short`, `long` or `full` for less or more of it: `03/11/2026`, `3 November 2026`, or `Tuesday 3 November 2026`.
- `{when, time}` writes its time, `14:30` or `2:30 PM` as the language's clock goes; `{when, time, medium}` adds the seconds.
- `{price, currency}` writes an amount of money with its currency's symbol where the language puts it: `€12.50` in English, `12,50 €` in French.

From a script, give a date as a `DateTime`, `DateOnly` or `TimeOnly`, and money as a `Money`, an amount and a `Currency`:

```csharp
Localization.Format("save.when", ("when", DateTime.Now));
Localization.Format("shop.price", ("price", new Money(4.99m, Currency.GBP)));
```

```toml
[[entry]]
key = "save.when"
en = "Saved {when, date} at {when, time}"
fr = "Enregistré le {when, date, long} à {when, time}"
```

The forms can hold more forms inside them. To write a brace or a `#` as itself, put it in apostrophes, `'{'`, and write an apostrophe as two, `''`.

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

## Assets in each language

Some games need more than text translated: a voice line recorded in each language, or a sign painted with words on it. An **asset table** is a `.assettable` file, written the way a string table is, but each language names an asset instead of giving text:

```toml
format = 1

[[entry]]
key = "voice.intro"
en = "Voices/Intro.wav"
fr = "Voices/Intro_fr.wav"
```

A script asks for a key's asset in the game's language, and gets the nearest language's when that one has none, as text falls back:

```csharp
Audio.Play(Localization.Sound("voice.intro"));
var sign = Localization.Prefab("sign.welcome");
```

`Localization.Asset(key)` gives the asset's reference for anything else, to load as you need. Every asset table goes into every build, with each language's assets.

## Translating in a spreadsheet

Translators often work in a spreadsheet. Select a string table in the Project panel, and under its entries in the Inspector, **Export CSV** writes it as a CSV file beside it, `Menu.csv` for `Menu.strings`, with a row for each key and a column for each language, in the order your project lists them. Open it in any spreadsheet, such as LibreOffice Calc or Google Sheets, fill in the empty cells, and save it as CSV again with the same name. **Import CSV** then reads it back and says what it would change: each key it adds (`+`), changes (`~`, with the languages that changed) and takes away (`-`). **Apply** keeps it. The sheet replaces the table, so a row deleted from the sheet takes that key away.

The same works from the command line, for a build server or a script. `vox loc export` writes the sheet:

```bash
vox loc export Text/Menu.strings
```

`vox loc import` says what the sheet would change:

```bash
vox loc import Menu.csv Text/Menu.strings
```

When that is what you want, run it again with `--apply` to keep it.

### A sheet online

When your translators share a Google Sheet, or the sheet is published as a CSV file anywhere online, import it from its link. Paste the link into **Link** under the table's sheet in the Inspector, as your browser's address bar or **Share** gives it, and click **Import from Link**. Voxear fetches the sheet and says what it would change, as Import CSV does, for **Apply** to keep. The table keeps the link, so next time it's one click. For a Google Sheet, the tab in the link is the one read; anyone with the link must be able to view it.

`vox loc import` takes a link too, and keeps it in the table:

```bash
vox loc import "https://docs.google.com/spreadsheets/d/…/edit#gid=0" Text/Menu.strings --apply
```

## Which language the game starts in

The game starts in the language the player's computer is set to, when your project has it: on Windows, the first language in their language settings, and on Linux the one the system's `LANG` names. When your project doesn't have it, the game takes the nearest language it does have, so a player in Portugal (`pt-PT`) gets `pt` when you have it. Otherwise the game is in your own language, the first you listed.

A game sold on Steam starts in the language the player picked for it in their Steam library instead, when you ship Steamworks with it. Voxear doesn't include Steamworks, which you get from Valve under your own Steamworks agreement: download the Steamworks SDK from Valve's partner site, and put its redistributable library beside your game's executable, `steam_api64.dll` on Windows or `libsteam_api.so` on Linux, from the SDK's `redistributable_bin` folder. When the game starts through Steam, Voxear finds the library and asks Steam the game's language. Without it, or when Steam isn't running, nothing changes. To try it from the editor, set `VOXEAR_STEAMWORKS` to the library's path.

Once a player picks a language in your game, setting `Localization.Language` from your options menu, the game remembers it beside their saves and starts in it from then on, whatever their computer is set to.

A key with no text in the game's language falls back the same way: `pt-BR`, then `pt`, then your own language. A text you haven't translated yet still shows, in the language you wrote it in.

## Languages written right to left

Arabic, Hebrew, Persian and Urdu read from right to left, and so should your game's screens when it is in one of them. When the game's language is written right to left, its UI is laid out that way by itself: rows run from the right, a margin or padding on the left moves to the right, and text starts at the right edge. Arabic and Hebrew text is shaped and ordered as it should be in any language, with words of English inside it kept in their own order.

Something that shouldn't be mirrored, such as a map, a picture with text in it, or a diagram of a gamepad, keeps its own way round with `direction: ltr` in your sheets:

```css
.minimap {
    direction: ltr;
}
```

The key maps in a controls screen already do. To see your screens mirrored while you make them, add Arabic or Hebrew to your project's languages and switch to it.

## Text that won't fit

Translated text is often longer than English, and German can be a third longer. Before anything is translated, run **Pseudo-localise Text** from the command palette. Every text from your tables then shows accented, a third longer and in brackets, as `[Ƥļåý ŧĥé ĝåɱé !!!!]`, both in the editor and while you play. A label that cuts its text off, or a button that spills over, shows straight away. So does any text your game shows that isn't in a table, as it stays plain. Run the command again to go back.

## Missing text

A key that no table has shows as `⟦menu.play⟧` while you make the game, in the editor and in development builds, and the Console warns about it once. A shipped game shows the key itself instead.
