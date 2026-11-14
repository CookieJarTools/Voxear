---
order: 4
---

A **font** of your own is a `.ttf` or `.otf` file anywhere in your project. Drop it into the Project panel and it is an asset: your UI's style sheets name it, and each language can name the fonts its text needs, so Japanese or Arabic draws in a font that has its letters. Every font goes into every build.

## A font in a style sheet

Name the font by its path in `font-family`:

```css
.title {
    font-family: url("Fonts/Cinzel-Regular.ttf");
    font-size: 32px;
}
```

`sans` and `mono` are Voxear's own faces. Select the font in the Project panel to see a line in it in the Inspector, and which languages use it.

## A language's fonts

A font made for English has no Japanese letters. Give Japanese a font that has them:

1. Put the font into your project, such as `Fonts/NotoSansJP-Regular.otf`.
2. Open **Project Settings** in the rail, and its **Localization** tab. Each of your languages has a row.
3. Click **Add Font** beside `ja` and pick the font. Add more and they are tried in order.

`voxear.toml` keeps them, by GUID:

```toml
[localization]
languages = ["en", "ja", "ar"]

[localization.fonts]
ja = ["5f0c…"]
ar = ["Fonts/NotoNaskhArabic-Regular.ttf"]
```

## Which font draws a letter

Text is drawn in the first font of these that has each letter:

1. the element's own font, from its style's `font-family`;
2. the game's language's fonts, in order, then those of the languages it falls back to: `pt-BR`'s, then `pt`'s, then your project's own language's;
3. Voxear's faces, Inter and Noto Sans, or JetBrains Mono for `mono`;
4. whatever font the player's system has with it.

So a title in your own display font keeps it for the letters it has, and its Japanese words come from the Japanese font. To draw a whole language in one font, give the text no font of its own and give the language that font.
