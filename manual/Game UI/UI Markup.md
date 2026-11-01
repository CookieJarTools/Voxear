---
order: 2
---

A `.vui` file lists what is on screen, as elements inside elements, in a form like HTML. It always starts with `<vui>`:

```xml
<vui>
    <Style src="Menu.vss" />
    <Panel name="menu" class="menu">
        <Label class="title">Paused</Label>
        <Button name="resume" class="primary">Resume</Button>
        <Toggle name="music" value="true">Music</Toggle>
        <Slider name="volume" min="0" max="10" value="7" step="1" />
        <Button name="quit">Quit</Button>
    </Panel>
</vui>
```

## Attributes every element takes

| Attribute | What it does |
| --- | --- |
| `name` | How scripts find it, and how a style sheet picks it out with `#name` |
| `class` | One or more class names, separated by spaces, for style sheets to use |
| `style` | Style written right there, as `style="width: 200px"`. It wins over style sheets |

## The UI elements

| Element | What it is | Its own attributes |
| --- | --- | --- |
| `Panel` | A box holding other elements, stacked top to bottom | |
| `Label` | A line of text, written between its tags | |
| `Button` | A button, with its text between its tags. It can hold an `Icon` and a `Label` instead | |
| `TextField` | A box to type in | `value`, `placeholder`, `multiline="true"` |
| `Toggle` | A switch, with its words between its tags | `value="true"` to start on |
| `Slider` | A bar to drag a value along | `min` (0), `max` (1), `value`, `step` (0 for any value) |
| `Dropdown` | A list to pick one from, written as `<Option>` elements inside it | `value`, the option picked first, counting from 0 |
| `Segmented` | A row of buttons, one picked, written as `<Segment text="…" />` inside it | `value`, the one picked first |
| `Tabs` | Pages with a tab each, written as `<Tab text="…">` elements holding each page | `selected`, the page shown first |
| `Foldout` | A heading that opens and closes what is under it | `text`, `open="false"` to start closed |
| `ScrollView` | A box whose content scrolls when it does not fit | |
| `SplitView` | Two elements side by side, with a bar to drag between them | `direction="column"` to stack them, `size` of the first in pixels or as `"40%"` |
| `Icon` | One of Voxear's icons, drawn in the text colour | `icon`, as `"play"`, `"settings"` or `"check"` |
| `Style` | Not shown: links a style sheet to the element it is in | `src`, the sheet's path from this file's folder |

Most icons are the editor's own, such as `play`, `pause`, `check`, `close`, `plus`, `search`, `settings`, `eye`, `lock`, `info` and `alert`.

## Linking a style sheet

`<Style src="Menu.vss" />` styles the element it sits in and everything inside it. Put one at the top, directly in `<vui>`, to style the whole document. The path is from the `.vui` file's folder, so `UI/Menu.vui` with `src="Menu.vss"` reads `UI/Menu.vss`.

## Mistakes in markup

A misspelt element name, such as `<Lable>`, stops the whole document loading, and the Console says which line is wrong. A misspelt attribute is only a warning, and the rest still loads. Keep the Console open while you work on UI.

> [!NOTE]
> A few elements need code that scripts cannot reach yet: `ListView` and `TreeView` cannot be given items from C#, and `Image` cannot be given a picture.
