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

The editor reads your tables again whenever one changes.

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

## Which language the game starts in

The game starts in the language the player's computer is set to, when your project has it. When it doesn't, the game takes the nearest language your project has, so a player in Portugal (`pt-PT`) gets `pt` when you have it. Otherwise the game is in your own language, the first you listed.

A key with no text in the game's language falls back the same way: `pt-BR`, then `pt`, then your own language. A text you haven't translated yet still shows, in the language you wrote it in.

> [!NOTE]
> On Windows the game starts in your own language for now. Reading Windows' language settings comes in a later version of Voxear.

## Missing text

A key that no table has shows as `⟦menu.play⟧` while you make the game, in the editor and in development builds, and the Console warns about it once. A shipped game shows the key itself instead.
